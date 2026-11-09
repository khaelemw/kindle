//! Reading the game's own files out of its archives (Data0-3.bdt, DLC.bdt), for files Kindle changes
//! instead of shipping (the maps' event files, files.rs), and Elden Ring's DCX packing.
//!
//! The game has each archive's header (`DataN.bhd`, RSA-encrypted on disk) decrypted in memory, in its
//! archive device (files.rs): Kindle reads a file's index entry there (`{u64 path hash, i32 padded
//! size, i32 size, i64 offset, i64 hash ranges, i64 AES ranges}`, the ranges' offsets from the
//! header's start), then the file's bytes from the archive's `.bdt` at its offset, and decrypts the
//! entry's AES ranges: at the header's start + the offset, a 16-byte AES-128 key, an i32 count and
//! that many `{i64 start, i64 end}` byte ranges, ECB-encrypted (start -1: unused). Checked against
//! the game's in memory (an event file's entry and key as an unpacking script reads them from
//! the `.bhd`, 2026-10-30). A size of 0 means the padded size.
//!
//! DCX: Elden Ring's files are packed with Oodle Kraken (`KRAK`) behind a 0x4C-byte header (sizes
//! big-endian at +0x1C and +0x20, the data from +0x4C). Kindle uses the game's own Oodle
//! (oo2core_6_win64.dll, loaded by the game) both ways; it packs with a seek reset every chunk, as
//! the game decompresses a big file's chunks in parallel (an unchanged repack without them crashed it).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

use kindle_core::memory;

/// An index entry's fields.
pub const ENTRY_PADDED: usize = 0x08;
pub const ENTRY_SIZE_FIELD: usize = 0x0C;
pub const ENTRY_OFFSET: usize = 0x10;
/// Most AES ranges one entry has (the game's have one or a few).
const MOST_RANGES: usize = 64;
/// Largest file Kindle reads out of an archive.
const MOST_BYTES: usize = 256 << 20;

/// Where a file is: its archive's `.bdt`, its offset and length there, and its AES key and ranges.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub offset: u64,
    /// The file's length, and what the archive stores of it (padded: the AES ranges may reach it).
    pub len: usize,
    pub padded: usize,
    pub key: Option<[u8; 16]>,
    pub ranges: Vec<(i64, i64)>,
}

/// Reads a file's place from an index entry (`entry`, in the header at `base`), its AES ranges at
/// `aes` (an offset from `base`, 0 for none: given apart, as Kindle clears the entry's own for a file
/// it serves).
pub fn stored(base: usize, entry: usize, aes: u64) -> Result<Stored, String> {
    let unreadable = || "the archive's index is unreadable".to_string();
    let padded = memory::read::<i32>(entry + ENTRY_PADDED).ok_or_else(unreadable)?;
    let size = memory::read::<i32>(entry + ENTRY_SIZE_FIELD).ok_or_else(unreadable)?;
    let offset = memory::read_u64(entry + ENTRY_OFFSET).ok_or_else(unreadable)?;
    let in_range = |n: i32| usize::try_from(n).ok().filter(|&l| l <= MOST_BYTES).ok_or("the file's size is out of range");
    let padded = in_range(padded)?;
    let len = if size > 0 { in_range(size)? } else { padded };
    let (mut key, mut ranges) = (None, Vec::new());
    if aes != 0 {
        let at = base + aes as usize;
        key = Some(memory::read::<[u8; 16]>(at).ok_or_else(unreadable)?);
        let count = memory::read::<i32>(at + 16).ok_or_else(unreadable)?.clamp(0, MOST_RANGES as i32) as usize;
        for i in 0..count {
            ranges.push(memory::read::<(i64, i64)>(at + 20 + 16 * i).ok_or_else(unreadable)?);
        }
    }
    Ok(Stored { offset, len, padded: padded.max(len), key, ranges })
}

/// The file's bytes as the game reads them: read from `bdt` and decrypted.
pub fn read(bdt: &Path, s: &Stored) -> Result<Vec<u8>, String> {
    let mut f = std::fs::File::open(bdt).map_err(|e| format!("{}: {e}", bdt.display()))?;
    f.seek(SeekFrom::Start(s.offset)).map_err(|e| e.to_string())?;
    let mut data = vec![0u8; s.padded];
    f.read_exact(&mut data).map_err(|e| format!("{}: {e}", bdt.display()))?;
    decrypt(s, &mut data);
    data.truncate(s.len);
    Ok(data)
}

/// Decrypts a file's AES ranges in place.
pub fn decrypt(s: &Stored, data: &mut [u8]) {
    let Some(key) = s.key else { return };
    let aes = kindle_core::aes::Aes128::new(&key);
    for &(start, end) in &s.ranges {
        if start < 0 || end <= start || end as usize > data.len() {
            continue;
        }
        for block in data[start as usize..end as usize].as_chunks_mut::<16>().0 {
            aes.decrypt_block(block);
        }
    }
}

/// The archive file next to the game for an archive's root ("data0" → Data0.bdt, "dlc" → DLC.bdt),
/// matched without regard to case.
pub fn bdt_of(game_dir: &Path, root: &str) -> Option<std::path::PathBuf> {
    let want = format!("{root}.bdt").to_lowercase();
    std::fs::read_dir(game_dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == want))
}

// ── DCX ──────────────────────────────────────────────────────────────────────

const DCX_HEADER: usize = 0x4C;
const KRAKEN: i32 = 8;
const LEVEL: i32 = 4;
/// Oodle's compress options: its defaults, copied, with seekChunkReset (+8) set.
const OPTIONS_SIZE: usize = 64;
const SEEK_CHUNK_RESET: usize = 8;

type Decompress = unsafe extern "C" fn(*const u8, isize, *mut u8, isize, i32, i32, i32, usize, isize, usize, usize, usize, isize, i32) -> isize;
type Compress = unsafe extern "C" fn(i32, *const u8, isize, *mut u8, i32, *const u8, usize, usize, usize, isize) -> isize;
type Defaults = unsafe extern "C" fn(i32, i32) -> *const u8;
type FarProc = unsafe extern "system" fn() -> isize;

struct Oodle {
    decompress: Decompress,
    compress: Compress,
    defaults: Defaults,
}

fn oodle() -> Result<&'static Oodle, String> {
    static FOUND: OnceLock<Option<Oodle>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
            use windows::core::{s, w};
            let dll = unsafe { GetModuleHandleW(w!("oo2core_6_win64.dll")) }.ok()?;
            let decompress = unsafe { GetProcAddress(dll, s!("OodleLZ_Decompress")) }?;
            let compress = unsafe { GetProcAddress(dll, s!("OodleLZ_Compress")) }?;
            let defaults = unsafe { GetProcAddress(dll, s!("OodleLZ_CompressOptions_GetDefault")) }?;
            Some(unsafe {
                Oodle {
                    decompress: std::mem::transmute::<FarProc, Decompress>(decompress),
                    compress: std::mem::transmute::<FarProc, Compress>(compress),
                    defaults: std::mem::transmute::<FarProc, Defaults>(defaults),
                }
            })
        })
        .as_ref()
        .ok_or_else(|| "the game's Oodle isn't loaded".into())
}

/// A DCX file's contents (Kraken).
pub fn undcx(dcx: &[u8]) -> Result<Vec<u8>, String> {
    if dcx.get(..4) != Some(b"DCX\0") || dcx.get(0x28..0x2C) != Some(b"KRAK") {
        return Err("not a DCX (Kraken) file".into());
    }
    let be = |at: usize| u32::from_be_bytes(dcx[at..at + 4].try_into().expect("4 bytes")) as usize;
    let (size, packed) = (be(0x1C), be(0x20));
    let data = dcx.get(DCX_HEADER..DCX_HEADER + packed).ok_or("the DCX file is cut short")?;
    if size > MOST_BYTES {
        return Err("the DCX file is too big".into());
    }
    let o = oodle()?;
    let mut out = vec![0u8; size];
    let n = unsafe { (o.decompress)(data.as_ptr(), data.len() as isize, out.as_mut_ptr(), size as isize, 1, 0, 0, 0, 0, 0, 0, 0, 0, 3) };
    if n != size as isize {
        return Err(format!("Oodle unpacked {n} of {size} bytes"));
    }
    Ok(out)
}

/// `data` as a DCX (Kraken) file with `like`'s header (a DCX file of the game's), its sizes changed.
pub fn dcx(like: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let header = like.get(..DCX_HEADER).filter(|h| h.starts_with(b"DCX\0") && h[0x28..0x2C] == *b"KRAK").ok_or("not a DCX (Kraken) file")?;
    let o = oodle()?;
    let mut options = memory::read_bytes(unsafe { (o.defaults)(KRAKEN, LEVEL) } as usize, OPTIONS_SIZE).ok_or("Oodle's options are unreadable")?;
    options[SEEK_CHUNK_RESET..SEEK_CHUNK_RESET + 4].copy_from_slice(&1i32.to_le_bytes());
    let mut packed = vec![0u8; data.len() + 65536 + data.len() / 8];
    let n = unsafe { (o.compress)(KRAKEN, data.as_ptr(), data.len() as isize, packed.as_mut_ptr(), LEVEL, options.as_ptr(), 0, 0, 0, 0) };
    if n <= 0 {
        return Err("Oodle couldn't pack the file".into());
    }
    packed.truncate(n as usize);
    let mut out = header.to_vec();
    out[0x1C..0x20].copy_from_slice(&(data.len() as u32).to_be_bytes());
    out[0x20..0x24].copy_from_slice(&(packed.len() as u32).to_be_bytes());
    out.extend_from_slice(&packed);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An entry and its key laid out as the game's header has them: an encrypted file read back plain.
    #[test]
    fn a_files_ranges_are_decrypted_by_its_entrys_key() {
        let key = [7u8; 16];
        let plain: Vec<u8> = (0..48u8).collect();
        let mut stored_bytes = plain.clone();
        let aes = kindle_core::aes::Aes128::new(&key);
        for block in stored_bytes[16..48].as_chunks_mut::<16>().0 {
            aes.encrypt_block(block);
        }
        let dir = std::env::temp_dir().join(format!("kindle-er-archive-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bdt = dir.join("Data0.bdt");
        std::fs::write(&bdt, [&[0xEE; 100][..], &stored_bytes].concat()).unwrap();
        // The header: the entry at 0, its key and one range (16..48) and an unused one at 0x40.
        let mut header = [0u8; 0x80];
        header[ENTRY_PADDED..ENTRY_PADDED + 4].copy_from_slice(&48i32.to_le_bytes());
        header[ENTRY_SIZE_FIELD..ENTRY_SIZE_FIELD + 4].copy_from_slice(&40i32.to_le_bytes());
        header[ENTRY_OFFSET..ENTRY_OFFSET + 8].copy_from_slice(&100u64.to_le_bytes());
        header[0x40..0x50].copy_from_slice(&key);
        header[0x50..0x54].copy_from_slice(&2i32.to_le_bytes());
        header[0x54..0x5C].copy_from_slice(&16i64.to_le_bytes());
        header[0x5C..0x64].copy_from_slice(&48i64.to_le_bytes());
        header[0x64..0x6C].copy_from_slice(&(-1i64).to_le_bytes());
        header[0x6C..0x74].copy_from_slice(&(-1i64).to_le_bytes());
        let base = header.as_ptr() as usize;
        let s = stored(base, base, 0x40).unwrap();
        assert_eq!((s.offset, s.len, s.padded, s.ranges.len()), (100, 40, 48, 2));
        // The whole range decrypted, then cut to the file's length.
        assert_eq!(read(&bdt, &s).unwrap(), plain[..40]);
        assert_eq!(bdt_of(&dir, "DATA0"), Some(bdt.clone()));
        assert_eq!(bdt_of(&dir, "data1"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dcx_files_are_checked_before_oodle_is_asked() {
        assert!(undcx(b"not a dcx file at all, but long enough to have a format field").is_err());
        assert!(dcx(b"DCX\0", b"data").is_err());
    }
}
