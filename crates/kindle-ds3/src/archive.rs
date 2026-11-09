//! Reading files out of the game's own archives (`Data1.bdt` …), for game files Kindle edits
//! instead of shipping (menu movies, see menu_movies.rs).
//!
//! An archive is a header (`DataN.bhd`) and data (`DataN.bdt`) next to the exe. The header is
//! RSA-encrypted with FromSoftware's private keys and opened with the public keys in the exe
//! (PEM `RSA PUBLIC KEY` blocks in .rdata; the exe on disk is protected, so they are read from
//! the running game's memory, early: see [`watch_keys`]). Each block of the key's size decrypts (`c^e mod n`, raw) to
//! one byte less of plain text. The plain header is `BHD5`, with a bucket table at +0x10
//! (count, offset): each bucket {count, offset} lists 0x28-byte entries {u32 path hash, i32
//! padded size, i64 data offset, i64 sha offset, i64 aes offset, u64 size}. The path hash is
//! `h = h * 37 + c` over the lowercase path with a leading `/` (`/menu/02_000_ingametop.gfx`).
//! If the aes offset is set, it points at {16-byte AES-128 key, i32 count, count x {i64 start,
//! i64 end}}: those byte ranges of the data are AES-128-ECB encrypted (start -1: unused).
//! Verified 2026-10-04 against the 1.15.2 archives (Data1: menu movies, 8 keys in the exe).
//!
//! Only the header blocks a lookup touches are decrypted: a path's entry is in bucket
//! `hash % bucket count` (a few blocks instead of the whole 400 KB header).
//!
//! Game paths name their archive by mount: `data1:/menu/x.gfx` is `/menu/x.gfx` in Data1.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::memory;

const PEM_BEGIN: &[u8] = b"-----BEGIN RSA PUBLIC KEY-----";
const PEM_END: &[u8] = b"-----END RSA PUBLIC KEY-----";
const ENTRY_SIZE: usize = 0x28;

/// The exe's RSA public keys found so far (the game has them in memory only while it starts).
static KEYS: Mutex<Vec<&'static RsaKey>> = Mutex::new(Vec::new());
/// Archive headers by archive name.
static HEADERS: Mutex<Option<HashMap<String, Arc<Header>>>> = Mutex::new(None);

/// The archives a game file can be in, in the order the game mounts them.
pub const ARCHIVES: [&str; 7] = ["data1", "data2", "data3", "data4", "data5", "dlc1", "dlc2"];

/// The file at `path` from whichever archive holds it: (archive, contents).
pub fn read_any(game_dir: &Path, path: &str) -> Result<(&'static str, Vec<u8>), String> {
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    for name in ARCHIVES {
        if !archive_path(game_dir, name, "bhd").is_file() {
            continue;
        }
        if let Ok(data) = read(game_dir, name, &path) {
            return Ok((name, data));
        }
    }
    Err(format!("{path} isn't in the game's archives"))
}

/// The file at `path` (e.g. `/menu/02_000_ingametop.gfx`) in archive `name` (e.g. `data1`)
/// of the game in `game_dir`, decrypted.
pub fn read(game_dir: &Path, name: &str, path: &str) -> Result<Vec<u8>, String> {
    let header = header(game_dir, name)?;
    let entry = find(header.as_ref(), path_hash(path)).ok_or_else(|| format!("{path} isn't in {name}"))?;
    let bdt = archive_path(game_dir, name, "bdt");
    let mut data = vec![0u8; entry.padded];
    let mut f = std::fs::File::open(&bdt).map_err(|e| format!("{}: {e}", bdt.display()))?;
    f.seek(SeekFrom::Start(entry.offset)).and_then(|_| f.read_exact(&mut data)).map_err(|e| format!("{}: {e}", bdt.display()))?;
    if entry.aes != 0 {
        crypt_ranges(header.as_ref(), entry.aes, &mut data, false)?;
    }
    if entry.size != 0 && entry.size <= data.len() {
        data.truncate(entry.size);
    }
    Ok(data)
}

/// `data` as the archive stores the file at `path` in archive `name`, which is how the game's
/// archive file reads come back (the loaders decrypt afterwards): padded with zeros to the
/// stored file's length (or to its own, rounded up to 16, if it's longer) and encrypted over the
/// same ranges with the same key. A longer file needs its size changed in the game's index (see
/// [`CheckHash::entry_at`]): the game then reads past the original, into whatever follows it in
/// the archive's data file, and Kindle's bytes replace all of it (see [`read_offset`] for a file
/// near the data file's end).
pub fn as_stored(game_dir: &Path, name: &str, path: &str, mut data: Vec<u8>) -> Result<Vec<u8>, String> {
    let header = header(game_dir, name)?;
    let entry = find(header.as_ref(), path_hash(path)).ok_or_else(|| format!("{path} isn't in {name}"))?;
    let padded = if data.len() > entry.padded { data.len().div_ceil(16) * 16 } else { entry.padded };
    data.resize(padded, 0);
    if entry.aes != 0 {
        crypt_ranges(header.as_ref(), entry.aes, &mut data, true)?;
    }
    Ok(data)
}

/// A DCX (DFLT) file's contents.
pub fn undcx(dcx: &[u8]) -> Option<Vec<u8>> {
    if dcx.get(..4)? != b"DCX\0" || dcx.get(0x28..0x2C)? != b"DFLT" {
        return None;
    }
    let size = u32::from_be_bytes(dcx.get(0x1C..0x20)?.try_into().ok()?) as usize;
    let mut data = miniz_oxide::inflate::decompress_to_vec_zlib(dcx.get(0x4C..)?).ok()?;
    data.truncate(size);
    Some(data)
}

/// `data` as a DCX (DFLT) file with `header`'s header (new sizes), meant to fit in `room`
/// bytes: miniz's quick compression if that fits, else Kindle's slower, tighter one.
pub fn dcx(header: &[u8], data: &[u8], room: usize) -> Option<Vec<u8>> {
    let mut out = header.get(..0x4C)?.to_vec();
    let mut z = miniz_oxide::deflate::compress_to_vec_zlib(data, 10);
    if out.len() + z.len() > room {
        z = crate::deflate::zlib(data);
    }
    out[0x1C..0x20].copy_from_slice(&(data.len() as u32).to_be_bytes());
    out[0x20..0x24].copy_from_slice(&(z.len() as u32).to_be_bytes());
    out.extend_from_slice(&z);
    Some(out)
}

/// Where the game should read `len` bytes for the file at `path` (its offset in the archive's
/// data file): its own offset, unless that runs past the data file's end (a longer replacement
/// of the last file), then early enough to end there. What's read is never used (Kindle's
/// bytes replace all of it); it only has to exist. Err if the data file is shorter than `len`.
pub fn read_offset(game_dir: &Path, name: &str, path: &str, len: usize) -> Result<u64, String> {
    let header = header(game_dir, name)?;
    let entry = find(header.as_ref(), path_hash(path)).ok_or_else(|| format!("{path} isn't in {name}"))?;
    let bdt = archive_path(game_dir, name, "bdt");
    let end = std::fs::metadata(&bdt).map_err(|e| format!("{}: {e}", bdt.display()))?.len();
    if entry.offset + len as u64 <= end {
        return Ok(entry.offset);
    }
    end.checked_sub(len as u64).map(|o| o / 16 * 16).ok_or_else(|| format!("{len} bytes, more than all of {name}"))
}

/// Whether the game's archives hold the file at `path` ("/chr/c1100.chrbnd.dcx"); by the
/// archives' headers, nothing read.
pub fn exists(game_dir: &Path, path: &str) -> bool {
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    ARCHIVES.iter().any(|name| archive_path(game_dir, name, "bhd").is_file() && stored_len(game_dir, name, &path).is_ok())
}

/// How many bytes the archive keeps for the file at `path`.
pub fn stored_len(game_dir: &Path, name: &str, path: &str) -> Result<usize, String> {
    let header = header(game_dir, name)?;
    find(header.as_ref(), path_hash(path)).map(|e| e.padded).ok_or_else(|| format!("{path} isn't in {name}"))
}

fn archive_path(game_dir: &Path, name: &str, ext: &str) -> PathBuf {
    game_dir.join(format!("{name}.{ext}"))
}

fn header(game_dir: &Path, name: &str) -> Result<Arc<Header>, String> {
    let key = name.to_lowercase();
    let mut guard = HEADERS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(h) = map.get(&key) {
        return Ok(h.clone());
    }
    let bhd = archive_path(game_dir, name, "bhd");
    let raw = std::fs::read(&bhd).map_err(|e| format!("{}: {e}", bhd.display()))?;
    let opens = |k: &&'static RsaKey| k.decrypt_block(&raw[..k.size.min(raw.len())]).starts_with(b"BHD5");
    let mut key_for = known_keys().into_iter().find(opens);
    if key_for.is_none() {
        add_keys(game_keys());
        key_for = known_keys().into_iter().find(opens);
    }
    let key_for = key_for.ok_or_else(|| format!("no key opens {}", bhd.display()))?;
    let header = Arc::new(Header { key: key_for, raw, blocks: Mutex::new(HashMap::new()) });
    map.insert(key, header.clone());
    Ok(header)
}

/// Header bytes by plain-text position.
trait HeaderBytes {
    fn bytes(&self, at: usize, len: usize) -> Option<Vec<u8>>;
}

impl HeaderBytes for Vec<u8> {
    fn bytes(&self, at: usize, len: usize) -> Option<Vec<u8>> {
        self.get(at..at + len).map(<[u8]>::to_vec)
    }
}

/// An encrypted archive header, decrypted a block at a time as lookups need it.
struct Header {
    raw: Vec<u8>,
    key: &'static RsaKey,
    blocks: Mutex<HashMap<usize, Vec<u8>>>,
}

impl HeaderBytes for Header {
    fn bytes(&self, at: usize, len: usize) -> Option<Vec<u8>> {
        let (size, plain) = (self.key.size, self.key.size - 1);
        let mut blocks = self.blocks.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::with_capacity(len);
        let mut p = at;
        while out.len() < len {
            let n = p / plain;
            let block = match blocks.entry(n) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => e.insert(self.key.decrypt_block(self.raw.get(n * size..(n + 1) * size)?)),
            };
            let (off, take) = (p % plain, (plain - p % plain).min(len - out.len()));
            out.extend_from_slice(&block[off..off + take]);
            p += take;
        }
        Some(out)
    }
}

/// The home bucket of `hash` in archive `name`'s index as the file has it: (bucket count, the
/// bucket's index, its entries' bytes). A copy of the index in memory holds the same.
pub fn home_bucket(game_dir: &Path, name: &str, hash: u32) -> Result<(usize, usize, Vec<u8>), String> {
    let header = header(game_dir, name)?;
    let h = header.as_ref();
    let head = h.bytes(0, 0x18).ok_or("archive header cut short")?;
    let (buckets, table) = (u32_at(&head, 0x10).ok_or("archive header cut short")? as usize, u32_at(&head, 0x14).ok_or("archive header cut short")? as usize);
    if buckets == 0 {
        return Err("an archive index with no buckets".into());
    }
    let k = hash as usize % buckets;
    let b = h.bytes(table + 8 * k, 8).ok_or("archive bucket cut short")?;
    let (count, at) = (u32_at(&b, 0).ok_or("archive bucket cut short")? as usize, u32_at(&b, 4).ok_or("archive bucket cut short")? as usize);
    Ok((buckets, k, h.bytes(at, ENTRY_SIZE * count).ok_or("archive bucket cut short")?))
}

pub fn path_hash(path: &str) -> u32 {
    path.to_lowercase().replace('\\', "/").chars().fold(0u32, |h, c| h.wrapping_mul(37).wrapping_add(c as u32))
}

#[derive(Debug, PartialEq)]
struct Entry {
    /// Where the entry is in the index.
    at: usize,
    padded: usize,
    offset: u64,
    sha: usize,
    aes: usize,
    size: usize,
}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], p: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(p..p + 8)?.try_into().ok()?))
}

/// The entry for `hash`: in its own bucket, else (an unusual header) in any.
fn find(header: &dyn HeaderBytes, hash: u32) -> Option<Entry> {
    let head = header.bytes(0, 0x18)?;
    if !head.starts_with(b"BHD5") {
        return None;
    }
    let (buckets, table) = (u32_at(&head, 0x10)? as usize, u32_at(&head, 0x14)? as usize);
    if buckets == 0 {
        return None;
    }
    let home = hash as usize % buckets;
    find_in_bucket(header, table, home, hash).or_else(|| (0..buckets).filter(|&k| k != home).find_map(|k| find_in_bucket(header, table, k, hash)))
}

fn find_in_bucket(header: &dyn HeaderBytes, table: usize, bucket: usize, hash: u32) -> Option<Entry> {
    let b = header.bytes(table + 8 * bucket, 8)?;
    let (count, at) = (u32_at(&b, 0)? as usize, u32_at(&b, 4)? as usize);
    let entries = header.bytes(at, ENTRY_SIZE * count)?;
    let (k, e) = entries.as_chunks::<ENTRY_SIZE>().0.iter().enumerate().find(|(_, e)| u32_at(*e, 0) == Some(hash))?;
    Some(Entry {
        at: at + ENTRY_SIZE * k,
        padded: u32_at(e, 4)? as usize,
        offset: u64_at(e, 8)?,
        sha: u64_at(e, 0x10)? as usize,
        aes: u64_at(e, 0x18)? as usize,
        size: u64_at(e, 0x20)? as usize,
    })
}

/// A file's check hash in its archive's index.
pub struct CheckHash {
    /// As the archive has it.
    pub old: [u8; 32],
    /// As it would be for the replacement.
    pub new: [u8; 32],
    /// Where it sits in the index (from the index's start).
    pub at: usize,
    /// Where the file's entry sits in the index: its padded size is the i32 at +4, its data
    /// offset the u64 at +8, its size the u64 at +0x20 (they change for a longer replacement).
    pub entry_at: usize,
    /// The index's first 16 bytes ("BHD5", two words, its size): a copy of the index in memory
    /// starts with them.
    pub header: Vec<u8>,
}

/// Whether the archive encrypts part of the file at `path` (its entry has a key).
pub fn encrypted(game_dir: &Path, name: &str, path: &str) -> Result<bool, String> {
    let header = header(game_dir, name)?;
    find(header.as_ref(), path_hash(path)).map(|e| e.aes != 0).ok_or_else(|| format!("{path} isn't in {name}"))
}

/// The game's check hash for the file at `path` in archive `name`, as the archive has it and as
/// it would be for `stored` (bytes as the archive stores them, see [`as_stored`]) followed by
/// zeros to `padded` bytes: SHA-256 of those bytes over the entry's hash ranges, then the archive's
/// salt (e.g. "FDP_map"). Matched against the game's own files (High Wall's map file, Data5). None
/// if the file has no hash.
pub fn check_hash(game_dir: &Path, name: &str, path: &str, stored: &[u8], padded: usize) -> Result<Option<CheckHash>, String> {
    let header = header(game_dir, name)?;
    let h = header.as_ref();
    let entry = find(h, path_hash(path)).ok_or_else(|| format!("{path} isn't in {name}"))?;
    if entry.sha == 0 {
        return Ok(None);
    }
    let head = h.bytes(entry.sha, 36).ok_or("archive hash cut short")?;
    let old: [u8; 32] = head[..32].try_into().expect("32 bytes");
    let count = u32_at(&head, 32).ok_or("archive hash cut short")? as usize;
    let ranges = h.bytes(entry.sha + 36, 16 * count).ok_or("archive hash ranges cut short")?;
    let salt_len = u32_at(&h.bytes(0x18, 4).ok_or("archive header cut short")?, 0).ok_or("archive header cut short")? as usize;
    let salt = h.bytes(0x1C, salt_len).ok_or("archive salt cut short")?;
    let mut spans = Vec::new();
    for r in ranges.as_chunks::<16>().0 {
        spans.push((u64_at(r, 0).ok_or("archive range cut short")? as usize, u64_at(r, 8).ok_or("archive range cut short")? as usize));
    }
    let mut hashed = hashed_ranges(&spans, stored, padded);
    hashed.extend_from_slice(&salt);
    let header = h.bytes(0, 16).ok_or("archive header cut short")?;
    Ok(Some(CheckHash { old, new: sha256(&hashed), at: entry.sha, entry_at: entry.at, header }))
}

/// The bytes of `spans` ([start, end) each) of `stored` followed by zeros to `padded`, one span
/// after the other (a span past the end, or the part of one past it, gives nothing).
fn hashed_ranges(spans: &[(usize, usize)], stored: &[u8], padded: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for &(start, end) in spans {
        let end = end.min(padded.max(stored.len()));
        if start >= end {
            continue;
        }
        out.extend_from_slice(stored.get(start..end.min(stored.len())).unwrap_or(&[]));
        out.resize(out.len() + end.saturating_sub(start.max(stored.len())), 0);
    }
    out
}

/// The first 16 bytes of an archive's index ("BHD5", two words, its size), which a copy of the
/// index in the game's memory starts with (as [`CheckHash::header`]).
pub fn index_header(game_dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let header = header(game_dir, name)?;
    header.as_ref().bytes(0, 16).ok_or_else(|| "archive header cut short".into())
}

/// SHA-256 (FIPS 180-4).
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// AES-128-ECB over the entry's ranges (the key and ranges at `at`), decrypting or encrypting.
fn crypt_ranges(header: &dyn HeaderBytes, at: usize, data: &mut [u8], encrypt: bool) -> Result<(), String> {
    let head = header.bytes(at, 20).ok_or("archive key cut short")?;
    let key: [u8; 16] = head[..16].try_into().expect("16 bytes");
    let count = u32_at(&head, 16).ok_or("archive key cut short")? as usize;
    let ranges = header.bytes(at + 20, 16 * count).ok_or("archive ranges cut short")?;
    let aes = kindle_core::aes::Aes128::new(&key);
    for r in ranges.as_chunks::<16>().0 {
        let (start, end) = (u64_at(r, 0).ok_or("archive range cut short")? as i64, u64_at(r, 8).ok_or("archive range cut short")? as i64);
        if start < 0 || end as usize > data.len() {
            continue;
        }
        for block in data[start as usize..end as usize].as_chunks_mut::<16>().0 {
            if encrypt {
                aes.encrypt_block(block);
            } else {
                aes.decrypt_block(block);
            }
        }
    }
    Ok(())
}

/// The archive keys found in an earlier session (kindle\cache, this PC's own; each line a key's
/// modulus in hex and its exponent): with them the watch below needn't run. Watching scanned the
/// game's .rdata (31 MB) over and over, a core busy for the first 13 s of every start, 30 s
/// after loading in for a game without a DLC (verified 2026-10-11). A key read back is used only
/// if it opens an archive's header, as one found in the game.
static KEY_CACHE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn set_key_cache(path: std::path::PathBuf) {
    if let Ok(text) = std::fs::read_to_string(&path) {
        let keys: Vec<RsaKey> = text
            .lines()
            .filter_map(|l| {
                let (n, e) = l.trim().split_once(' ')?;
                let n: Vec<u8> = (0..n.len() / 2).map(|i| u8::from_str_radix(n.get(2 * i..2 * i + 2)?, 16).ok()).collect::<Option<_>>()?;
                (!n.is_empty() && n.last()? & 1 == 1).then(|| RsaKey { size: n.len(), modulus: Mont::new(&n), exponent: e.parse().ok().unwrap_or(0), n })
            })
            .filter(|k| k.exponent != 0)
            .collect();
        if !keys.is_empty() {
            crate::debug!("archive keys: {} from the cache", keys.len());
            add_keys(keys);
        }
    }
    let _ = KEY_CACHE.set(path);
}

fn save_keys() {
    let Some(path) = KEY_CACHE.get() else { return };
    let text: String = known_keys().iter().map(|k| format!("{} {}\n", k.n.iter().map(|b| format!("{b:02x}")).collect::<String>(), k.exponent)).collect();
    if std::fs::read_to_string(path).ok().as_deref() != Some(text.as_str()) {
        let _ = path.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(path, text);
    }
}

/// Watches for the keys while the game starts (call as early as possible). The game has them in
/// .rdata only for a moment then (about a second in, verified 2026-10-06 with Kindle the only
/// DLL); some other DLLs keep them there longer, so a later read can also find them. They don't
/// all show at once: the base archives' keys show about a second in, the DLCs' only once the
/// title screen is passed (about 17 s in when pressed straight through, none while the game
/// waits there; verified 2026-10-07). So the watch goes on until every archive there opens, or
/// until a character has been in the world for a while (a DLC whose key never shows).
pub fn watch_keys() {
    const AFTER_LOAD: std::time::Duration = std::time::Duration::from_secs(30);
    std::thread::spawn(|| {
        let start = std::time::Instant::now();
        let game_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
        let all_open =
            || game_dir.as_deref().is_some_and(|dir| ARCHIVES.iter().all(|a| !archive_path(dir, a, "bhd").is_file() || header_with_known_keys(dir, a)));
        let mut loaded_at = None;
        loop {
            if !known_keys().is_empty() {
                if all_open() {
                    break;
                }
                if crate::game::player_loaded() && loaded_at.get_or_insert_with(std::time::Instant::now).elapsed() > AFTER_LOAD {
                    break;
                }
                // Some are in: a scan every few frames is enough for the rest.
                std::thread::sleep(std::time::Duration::from_millis(20));
            } else {
                // None yet (they show about a second in, for about a second): not a busy loop.
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let (Some((base, _)), Some((rva, size))) = (memory::game_module(), memory::section(".rdata")) else { return };
            // Always readable (the game's own image), unlike what the game allocates.
            let rdata = unsafe { std::slice::from_raw_parts((base + rva) as *const u8, size) };
            if rdata.windows(PEM_BEGIN.len()).any(|w| w[0] == b'-' && w == PEM_BEGIN) {
                add_keys(pem_keys(rdata));
            }
        }
        crate::debug!("archive keys: {} found ({:.1}s)", known_keys().len(), start.elapsed().as_secs_f32());
        save_keys();
        // (The keys from the cache: the watch ended at once, before the game has its archives'
        // indexes; the search when its data is ready finds them.)
        if start.elapsed() > std::time::Duration::from_secs(1) {
            crate::files::find_index_copies();
        }
    });
}

/// Whether the archive's header opens with the keys found so far.
fn header_with_known_keys(game_dir: &Path, name: &str) -> bool {
    if HEADERS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|m| m.contains_key(&name.to_lowercase())) {
        return true;
    }
    let Ok(raw) = std::fs::read(archive_path(game_dir, name, "bhd")) else { return false };
    // Opened: kept (header() keeps it), so it isn't read again.
    known_keys().iter().any(|k| k.decrypt_block(&raw[..k.size.min(raw.len())]).starts_with(b"BHD5")) && header(game_dir, name).is_ok()
}

fn known_keys() -> Vec<&'static RsaKey> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Keeps the keys not known yet (kept for the session).
fn add_keys(found: Vec<RsaKey>) {
    let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    for k in found {
        if !keys.iter().any(|h| h.n == k.n) {
            keys.push(Box::leak(Box::new(k)));
        }
    }
}

/// The RSA public keys in the running game's .rdata.
fn game_keys() -> Vec<RsaKey> {
    let (Some((base, _)), Some((rva, size))) = (memory::game_module(), memory::section(".rdata")) else { return Vec::new() };
    let Some(rdata) = memory::read_bytes(base + rva, size) else { return Vec::new() };
    pem_keys(&rdata)
}

fn pem_keys(data: &[u8]) -> Vec<RsaKey> {
    let mut keys = Vec::new();
    let mut pos = 0;
    while let Some(start) = find_bytes(&data[pos..], PEM_BEGIN).map(|i| pos + i + PEM_BEGIN.len()) {
        let Some(end) = find_bytes(&data[start..], PEM_END).map(|i| start + i) else { break };
        if let Some(key) = base64(&data[start..end]).and_then(|der| RsaKey::from_der(&der)) {
            keys.push(key);
        }
        pos = end;
    }
    keys
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn base64(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// A DER integer at `pos`: (big-endian bytes, position after it).
fn der_int(der: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    if *der.get(pos)? != 2 {
        return None;
    }
    let (len, start) = der_len(der, pos + 1)?;
    Some((der.get(start..start + len)?, start + len))
}

fn der_len(der: &[u8], pos: usize) -> Option<(usize, usize)> {
    let first = *der.get(pos)? as usize;
    if first & 0x80 == 0 {
        return Some((first, pos + 1));
    }
    let n = first & 0x7F;
    let len = der.get(pos + 1..pos + 1 + n)?.iter().fold(0usize, |v, &b| (v << 8) | b as usize);
    Some((len, pos + 1 + n))
}

/// An RSA public key, for raw (unpadded) decryption.
struct RsaKey {
    modulus: Mont,
    exponent: u64,
    /// Bytes per encrypted block.
    size: usize,
    /// The modulus as written (big endian), to tell keys apart.
    n: Vec<u8>,
}

impl RsaKey {
    /// PKCS#1 `RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }`.
    fn from_der(der: &[u8]) -> Option<Self> {
        if *der.first()? != 0x30 {
            return None;
        }
        let (_, body) = der_len(der, 1)?;
        let (n, next) = der_int(der, body)?;
        let (e, _) = der_int(der, next)?;
        let n: Vec<u8> = n.iter().copied().skip_while(|&b| b == 0).collect();
        if n.is_empty() || n.last()? & 1 == 0 || e.len() > 8 {
            return None;
        }
        Some(Self { size: n.len(), modulus: Mont::new(&n), exponent: e.iter().fold(0u64, |v, &b| (v << 8) | b as u64), n })
    }

    /// One block, `c^e mod n` as `size - 1` big-endian bytes.
    fn decrypt_block(&self, block: &[u8]) -> Vec<u8> {
        let m = self.modulus.pow(block, self.exponent);
        let bytes = self.modulus.to_be_bytes(&m, self.size);
        bytes[1..].to_vec()
    }
}

/// Modular arithmetic for an odd modulus, in Montgomery form (64-bit limbs, little endian).
struct Mont {
    n: Vec<u64>,
    /// -n^-1 mod 2^64.
    n0: u64,
    /// R^2 mod n, R = 2^(64 * limbs).
    r2: Vec<u64>,
}

impl Mont {
    fn new(modulus_be: &[u8]) -> Self {
        let n = limbs(modulus_be, modulus_be.len().div_ceil(8));
        let mut inv = 1u64;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(n[0].wrapping_mul(inv)));
        }
        let k = n.len();
        // R^2 mod n by doubling 1 (2 * 64 * k) times.
        let mut x = vec![0u64; k];
        x[0] = 1;
        for _ in 0..128 * k {
            let carry = shl1(&mut x);
            if carry || !less(&x, &n) {
                sub(&mut x, &n);
            }
        }
        Self { n0: inv.wrapping_neg(), r2: x, n }
    }

    fn mul(&self, a: &[u64], b: &[u64]) -> Vec<u64> {
        let k = self.n.len();
        let mut t = vec![0u64; k + 2];
        for &bi in b.iter().take(k) {
            let mut c = 0u128;
            for j in 0..k {
                let s = t[j] as u128 + a[j] as u128 * bi as u128 + c;
                t[j] = s as u64;
                c = s >> 64;
            }
            let s = t[k] as u128 + c;
            t[k] = s as u64;
            t[k + 1] = (s >> 64) as u64;
            let m = t[0].wrapping_mul(self.n0);
            let mut c = (t[0] as u128 + m as u128 * self.n[0] as u128) >> 64;
            for j in 1..k {
                let s = t[j] as u128 + m as u128 * self.n[j] as u128 + c;
                t[j - 1] = s as u64;
                c = s >> 64;
            }
            let s = t[k] as u128 + c;
            t[k - 1] = s as u64;
            t[k] = t[k + 1] + (s >> 64) as u64;
            t[k + 1] = 0;
        }
        let mut r = t[..k].to_vec();
        if t[k] != 0 || !less(&r, &self.n) {
            sub(&mut r, &self.n);
        }
        r
    }

    /// `base^e mod n` for a big-endian base; plain (not Montgomery) result.
    fn pow(&self, base_be: &[u8], e: u64) -> Vec<u64> {
        let k = self.n.len();
        let mut base = limbs(base_be, k.max(base_be.len().div_ceil(8)));
        base.truncate(k);
        while !less(&base, &self.n) {
            sub(&mut base, &self.n);
        }
        let b = self.mul(&base, &self.r2);
        let mut one = vec![0u64; k];
        one[0] = 1;
        let mut acc = self.mul(&one, &self.r2);
        for i in (0..64 - e.leading_zeros()).rev() {
            acc = self.mul(&acc, &acc);
            if e >> i & 1 == 1 {
                acc = self.mul(&acc, &b);
            }
        }
        self.mul(&acc, &one)
    }

    fn to_be_bytes(&self, v: &[u64], size: usize) -> Vec<u8> {
        let all: Vec<u8> = v.iter().rev().flat_map(|l| l.to_be_bytes()).collect();
        all[all.len() - size..].to_vec()
    }
}

fn limbs(be: &[u8], k: usize) -> Vec<u64> {
    let mut out = vec![0u64; k];
    for (i, &b) in be.iter().rev().enumerate() {
        if i / 8 < k {
            out[i / 8] |= (b as u64) << (8 * (i % 8));
        }
    }
    out
}

fn less(a: &[u64], b: &[u64]) -> bool {
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    false
}

fn sub(a: &mut [u64], b: &[u64]) {
    let mut borrow = 0u64;
    for i in 0..a.len() {
        let (d, o1) = a[i].overflowing_sub(b[i]);
        let (d, o2) = d.overflowing_sub(borrow);
        a[i] = d;
        borrow = (o1 | o2) as u64;
    }
}

fn shl1(a: &mut [u64]) -> bool {
    let mut carry = 0u64;
    for l in a.iter_mut() {
        let next = *l >> 63;
        *l = (*l << 1) | carry;
        carry = next;
    }
    carry == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_ranges_count_the_padding() {
        let data = [1u8, 2, 3];
        let mut padded = data.to_vec();
        padded.resize(8, 0);
        let spans = [(0, 2), (2, 8), (6, 12), (10, 12)];
        assert_eq!(hashed_ranges(&spans, &data, 8), hashed_ranges(&spans, &padded, padded.len()));
        assert_eq!(hashed_ranges(&spans, &data, 8), vec![1, 2, 3, 0, 0, 0, 0, 0, 0, 0]);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn sha256_matches_the_standard_examples() {
        assert_eq!(sha256(b"abc").to_vec(), hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        assert_eq!(sha256(b"").to_vec(), hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
        let two_blocks = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha256(two_blocks).to_vec(), hex("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"));
    }

    #[test]
    fn modular_powers_match() {
        let small = Mont::new(&3233u16.to_be_bytes());
        assert_eq!(small.pow(&855u16.to_be_bytes(), 17)[0], 1892);
        let n = hex("d23f0824128b2f330c5c7fd0a6a3a4506513270e269e0d37f2a74de452e6b439");
        let c = hex("00dbd9d781e74ef5e8e25d940ed904759531985d5d9dc9f81818e811892f902b");
        let m = Mont::new(&n);
        assert_eq!(m.to_be_bytes(&m.pow(&c, 65537), 32), hex("9124f1870b58ea3e9e877993ea7f516408d153836f6808d40215c2071e662305"));
    }

    #[test]
    fn keys_parse_from_pem() {
        // RSAPublicKey { n = 3233, e = 17 }
        let der = [0x30, 0x07, 0x02, 0x02, 0x0c, 0xa1, 0x02, 0x01, 0x11];
        let text: String = {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            der.chunks(3)
                .flat_map(|c| {
                    let v = c.iter().enumerate().fold(0u32, |v, (i, &b)| v | (b as u32) << (16 - 8 * i));
                    (0..4).map(move |i| if i <= c.len() { A[(v >> (18 - 6 * i) & 63) as usize] as char } else { '=' })
                })
                .collect()
        };
        let pem = format!("junk-----BEGIN RSA PUBLIC KEY-----\n{text}\n-----END RSA PUBLIC KEY-----junk");
        let keys = pem_keys(pem.as_bytes());
        assert_eq!(keys.len(), 1);
        assert_eq!((keys[0].size, keys[0].exponent), (2, 17));
    }

    #[test]
    fn encrypted_headers_read_across_blocks() {
        // n = 61 * 53, e = 17, d = 2753: each 2-byte block holds one plain byte.
        let n = 3233u16.to_be_bytes();
        let key: &'static RsaKey = Box::leak(Box::new(RsaKey { modulus: Mont::new(&n), exponent: 17, size: 2, n: n.to_vec() }));
        let raw: Vec<u8> = b"BHD5 lookup".iter().flat_map(|&b| (Mont::new(&n).pow(&[b], 2753)[0] as u16).to_be_bytes()).collect();
        let header = Header { raw, key, blocks: Mutex::new(HashMap::new()) };
        assert_eq!(header.bytes(0, 4).unwrap(), b"BHD5");
        assert_eq!(header.bytes(5, 6).unwrap(), b"lookup");
        assert_eq!(header.bytes(8, 4), None);
    }

    #[test]
    fn headers_find_entries() {
        let mut h = b"BHD5".to_vec();
        h.resize(0x18, 0);
        h[0x10..0x14].copy_from_slice(&1u32.to_le_bytes());
        h[0x14..0x18].copy_from_slice(&0x18u32.to_le_bytes());
        h.extend_from_slice(&1u32.to_le_bytes());
        h.extend_from_slice(&0x20u32.to_le_bytes());
        let hash = path_hash("/menu/02_000_IngameTop.gfx");
        h.extend_from_slice(&hash.to_le_bytes());
        h.extend_from_slice(&13680u32.to_le_bytes());
        h.extend_from_slice(&0x1234u64.to_le_bytes());
        h.extend_from_slice(&0u64.to_le_bytes());
        h.extend_from_slice(&0x99u64.to_le_bytes());
        h.extend_from_slice(&13676u64.to_le_bytes());
        assert_eq!(find(&h, hash), Some(Entry { at: 0x20, padded: 13680, offset: 0x1234, sha: 0, aes: 0x99, size: 13676 }));
        assert_eq!(find(&h, hash ^ 1), None);
        assert_eq!(path_hash("/A"), path_hash("/a"));
    }
}
