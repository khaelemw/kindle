//! A deflate (RFC 1951) encoder that spends time for size, for map files that have to fit back
//! into the archive slot of the game's own (whose encoder packs them tighter than miniz).
//!
//! Every position's matches are found first (for each length, a near distance), from a binary
//! tree of earlier positions per hash. The input
//! is then split into blocks where its statistics change, and each block's matches are chosen
//! as the cheapest path through it under the bit costs of the block's own codes, repeated as
//! the costs settle. Codes are length-limited Huffman codes (package-merge).

const WINDOW: usize = 32768;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// How many earlier positions a match search looks at, at most.
const MAX_DEPTH: usize = 256;
/// Rounds of choosing matches by the previous round's costs, per block.
const ITERATIONS: usize = 6;
/// Rounds without a smaller block before giving up on it.
const PATIENCE: usize = 2;
const MAX_BLOCKS: usize = 40;
/// The fewest symbols a block is split into.
const MIN_BLOCK: usize = 1024;

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order code length code lengths are written in.
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
const LITLEN_CODES: usize = 286;
const DIST_CODES: usize = 30;
const EOB: usize = 256;

/// `data` as a zlib stream (RFC 1950).
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0xDA];
    out.extend(deflate(data));
    out.extend(adler32(data).to_be_bytes());
    out
}

/// `data` as raw deflate.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let matches = Matches::find(data);
    let greedy = greedy(data, &matches);
    // Block boundaries, as positions in data.
    let mut cuts = Vec::new();
    split(&greedy, 0, greedy.len(), &mut cuts);
    cuts.sort_unstable();
    let mut bounds = vec![0];
    let (mut pos, mut k) = (0, 0);
    for (i, s) in greedy.iter().enumerate() {
        if cuts.get(k) == Some(&i) {
            bounds.push(pos);
            k += 1;
        }
        pos += s.bytes();
    }
    bounds.push(data.len());
    let mut bits = Bits::default();
    let mut sym = 0;
    for (b, w) in bounds.windows(2).enumerate() {
        // The greedy symbols of this block start its costs.
        let start = sym;
        let mut covered = 0;
        while sym < greedy.len() && covered < w[1] - w[0] {
            covered += greedy[sym].bytes();
            sym += 1;
        }
        let block = best_block(data, &matches, w[0], w[1], &greedy[start..sym]);
        write_block(&mut bits, &block, b == bounds.len() - 2);
    }
    bits.finish()
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

/// A literal (dist 0, len the byte) or a match.
#[derive(Clone, Copy, Debug)]
struct Sym {
    len: u16,
    dist: u16,
}

impl Sym {
    fn bytes(self) -> usize {
        if self.dist == 0 { 1 } else { self.len as usize }
    }
}

fn len_symbol(len: usize) -> usize {
    LEN_BASE.partition_point(|&b| b as usize <= len) - 1
}

fn dist_symbol(dist: usize) -> usize {
    DIST_BASE.partition_point(|&b| b as usize <= dist) - 1
}

/// For every position, (length, distance) pairs by increasing length: lengths above the
/// previous pair's, up to this one's, are found at this distance.
struct Matches {
    start: Vec<u32>,
    pairs: Vec<(u16, u16)>,
}

impl Matches {
    fn find(data: &[u8]) -> Self {
        let n = data.len();
        let hash = |i: usize| (u32::from_le_bytes([data[i], data[i + 1], data[i + 2], 0]).wrapping_mul(2654435761) >> 16) as usize;
        let mut head = vec![u32::MAX; 1 << 16];
        // Each hash's positions as a binary tree ordered by the bytes that follow them, the
        // newest at the root: son[2k] holds the smaller subtree of position k (mod WINDOW),
        // son[2k + 1] the larger.
        let mut son = vec![u32::MAX; 2 * WINDOW];
        let mut start = Vec::with_capacity(n + 1);
        let mut pairs = Vec::new();
        for i in 0..n {
            start.push(pairs.len() as u32);
            if i + MIN_MATCH > n {
                continue;
            }
            let h = hash(i);
            let max = (n - i).min(MAX_MATCH);
            let mut cand = head[h];
            head[h] = i as u32;
            let (mut smaller, mut larger) = (2 * (i % WINDOW), 2 * (i % WINDOW) + 1);
            let (mut len_s, mut len_l) = (0, 0);
            let mut best = MIN_MATCH - 1;
            let mut depth = MAX_DEPTH;
            loop {
                if cand == u32::MAX || i - cand as usize >= WINDOW || depth == 0 {
                    son[smaller] = u32::MAX;
                    son[larger] = u32::MAX;
                    break;
                }
                depth -= 1;
                let c = cand as usize;
                let node = 2 * (c % WINDOW);
                let skip = len_s.min(len_l);
                let l = skip + common(&data[c + skip..c + max], &data[i + skip..i + max]);
                if l > best {
                    best = l;
                    pairs.push((l as u16, (i - c) as u16));
                }
                if l == max {
                    son[smaller] = son[node];
                    son[larger] = son[node + 1];
                    break;
                }
                if data[c + l] < data[i + l] {
                    son[smaller] = cand;
                    smaller = node + 1;
                    cand = son[node + 1];
                    len_s = l;
                } else {
                    son[larger] = cand;
                    larger = node;
                    cand = son[node];
                    len_l = l;
                }
            }
        }
        start.push(pairs.len() as u32);
        Self { start, pairs }
    }

    fn at(&self, i: usize) -> &[(u16, u16)] {
        &self.pairs[self.start[i] as usize..self.start[i + 1] as usize]
    }
}

/// How many bytes a and b (as long as each other) start with in common.
fn common(a: &[u8], b: &[u8]) -> usize {
    let mut n = 0;
    while n + 8 <= a.len() {
        let x = u64::from_le_bytes(a[n..n + 8].try_into().unwrap()) ^ u64::from_le_bytes(b[n..n + 8].try_into().unwrap());
        if x != 0 {
            return n + (x.trailing_zeros() / 8) as usize;
        }
        n += 8;
    }
    n + a[n..].iter().zip(&b[n..]).take_while(|(x, y)| x == y).count()
}

/// The longest match at each position, else a literal.
fn greedy(data: &[u8], m: &Matches) -> Vec<Sym> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        match m.at(i).last() {
            Some(&(len, dist)) => {
                out.push(Sym { len, dist });
                i += len as usize;
            }
            None => {
                out.push(Sym { len: data[i] as u16, dist: 0 });
                i += 1;
            }
        }
    }
    out
}

struct Counts {
    litlen: [u32; LITLEN_CODES],
    dist: [u32; DIST_CODES],
}

fn counts(syms: &[Sym]) -> Counts {
    let mut c = Counts { litlen: [0; LITLEN_CODES], dist: [0; DIST_CODES] };
    c.litlen[EOB] = 1;
    for s in syms {
        if s.dist == 0 {
            c.litlen[s.len as usize] += 1;
        } else {
            c.litlen[257 + len_symbol(s.len as usize)] += 1;
            c.dist[dist_symbol(s.dist as usize)] += 1;
        }
    }
    c
}

/// Bits per symbol by entropy (unseen symbols cost as much as one seen once).
fn entropy(counts: &[u32]) -> Vec<f32> {
    let total: u32 = counts.iter().sum();
    let log_total = (total.max(1) as f32).log2();
    counts.iter().map(|&c| if c == 0 { log_total } else { log_total - (c as f32).log2() }).collect()
}

/// Splits syms[a..b] where coding the halves apart is smaller, recursively (indices into cuts).
fn split(syms: &[Sym], a: usize, b: usize, cuts: &mut Vec<usize>) {
    if b - a < 2 * MIN_BLOCK || cuts.len() + 1 >= MAX_BLOCKS {
        return;
    }
    let cost = |x: usize, y: usize| block_bits(&counts(&syms[x..y]));
    let whole = cost(a, b);
    let (mut lo, mut hi) = (a + MIN_BLOCK, b - MIN_BLOCK);
    let mut best = (u64::MAX, lo);
    while hi > lo {
        let step = ((hi - lo) / 16).max(1);
        let mut k = lo;
        while k <= hi {
            let c = cost(a, k) + cost(k, b);
            if c < best.0 {
                best = (c, k);
            }
            k += step;
        }
        if step == 1 {
            break;
        }
        lo = best.1.saturating_sub(step).max(a + MIN_BLOCK);
        hi = (best.1 + step).min(b - MIN_BLOCK);
    }
    if best.0 < whole {
        cuts.push(best.1);
        split(syms, a, best.1, cuts);
        split(syms, best.1, b, cuts);
    }
}

/// The smallest coding of data[s..e] found: matches chosen by cost, the costs taken from the
/// previous choice's statistics (starting with `seed`'s).
fn best_block(data: &[u8], m: &Matches, s: usize, e: usize, seed: &[Sym]) -> Vec<Sym> {
    let mut best = (block_bits(&counts(seed)), seed.to_vec());
    let mut stats = counts(seed);
    let mut stale = 0;
    for _ in 0..ITERATIONS {
        let syms = cheapest(data, m, s, e, &entropy(&stats.litlen), &entropy(&stats.dist));
        stats = counts(&syms);
        let bits = block_bits(&stats);
        if bits < best.0 {
            best = (bits, syms);
            stale = 0;
        } else {
            stale += 1;
            if stale >= PATIENCE {
                break;
            }
        }
    }
    best.1
}

/// The cheapest symbols for data[s..e] under the given bit costs (shortest path).
fn cheapest(data: &[u8], m: &Matches, s: usize, e: usize, litlen: &[f32], dist: &[f32]) -> Vec<Sym> {
    let n = e - s;
    let mut len_cost = [0f32; MAX_MATCH + 1];
    for (l, c) in len_cost.iter_mut().enumerate().skip(MIN_MATCH) {
        let ls = len_symbol(l);
        *c = litlen[257 + ls] + LEN_EXTRA[ls] as f32;
    }
    let mut cost = vec![f32::INFINITY; n + 1];
    let mut back = vec![(0u16, 0u16); n + 1];
    cost[0] = 0.0;
    for i in 0..n {
        let c = cost[i];
        let p = s + i;
        let lit = c + litlen[data[p] as usize];
        if lit < cost[i + 1] {
            cost[i + 1] = lit;
            back[i + 1] = (1, 0);
        }
        let mut lo = MIN_MATCH;
        for &(len, d) in m.at(p) {
            let hi = (len as usize).min(n - i);
            let ds = dist_symbol(d as usize);
            let dc = c + dist[ds] + DIST_EXTRA[ds] as f32;
            for l in lo..=hi {
                let t = dc + len_cost[l];
                if t < cost[i + l] {
                    cost[i + l] = t;
                    back[i + l] = (l as u16, d);
                }
            }
            if hi == n - i {
                break;
            }
            lo = hi + 1;
        }
    }
    let mut out = Vec::new();
    let mut i = n;
    while i > 0 {
        let (l, d) = back[i];
        if d == 0 {
            out.push(Sym { len: data[s + i - 1] as u16, dist: 0 });
            i -= 1;
        } else {
            out.push(Sym { len: l, dist: d });
            i -= l as usize;
        }
    }
    out.reverse();
    out
}

/// Code lengths of at most `limit` bits for symbol counts (package-merge); unused symbols get
/// 0. At least two symbols get a code, so the code is complete.
fn code_lengths(counts: &[u32], limit: u32) -> Vec<u8> {
    let mut lens = vec![0u8; counts.len()];
    let mut leaves: Vec<(u64, usize)> = counts.iter().enumerate().filter(|&(_, &c)| c > 0).map(|(s, &c)| (c as u64, s)).collect();
    if leaves.len() < 2 {
        let used = leaves.first().map_or(0, |l| l.1);
        lens[used] = 1;
        lens[if used == 0 { 1 } else { 0 }] = 1;
        return lens;
    }
    leaves.sort_unstable();
    enum Node {
        Leaf(usize),
        Pair(usize, usize),
    }
    let mut nodes: Vec<Node> = leaves.iter().map(|&(_, s)| Node::Leaf(s)).collect();
    let singles: Vec<(u64, usize)> = leaves.iter().enumerate().map(|(k, &(w, _))| (w, k)).collect();
    let mut list = singles.clone();
    for _ in 1..limit {
        let mut packages = Vec::with_capacity(list.len() / 2);
        for [a, b] in list.as_chunks::<2>().0 {
            nodes.push(Node::Pair(a.1, b.1));
            packages.push((a.0 + b.0, nodes.len() - 1));
        }
        let mut merged = Vec::with_capacity(singles.len() + packages.len());
        let (mut x, mut y) = (0, 0);
        while x < singles.len() || y < packages.len() {
            if y >= packages.len() || (x < singles.len() && singles[x].0 <= packages[y].0) {
                merged.push(singles[x]);
                x += 1;
            } else {
                merged.push(packages[y]);
                y += 1;
            }
        }
        list = merged;
    }
    let mut stack: Vec<usize> = list[..2 * leaves.len() - 2].iter().map(|&(_, n)| n).collect();
    while let Some(n) = stack.pop() {
        match nodes[n] {
            Node::Leaf(s) => lens[s] += 1,
            Node::Pair(a, b) => {
                stack.push(a);
                stack.push(b);
            }
        }
    }
    lens
}

/// Canonical codes for code lengths, bit-reversed for writing.
fn codes(lens: &[u8]) -> Vec<u16> {
    let mut count = [0u16; 16];
    for &l in lens.iter().filter(|&&l| l > 0) {
        count[l as usize] += 1;
    }
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            c.reverse_bits() >> (16 - l)
        })
        .collect()
}

/// A block's code lengths and its header's code length symbols.
struct Header {
    litlen: Vec<u8>,
    dist: Vec<u8>,
    /// (symbol, extra bits' value) for the code lengths, run-length coded.
    runs: Vec<(u8, u8)>,
    cl: Vec<u8>,
    hlit: usize,
    hdist: usize,
    hclen: usize,
}

fn header(c: &Counts) -> Header {
    let litlen = code_lengths(&c.litlen, 15);
    let dist = code_lengths(&c.dist, 15);
    let hlit = (litlen.iter().rposition(|&l| l > 0).unwrap_or(0) + 1).max(257);
    let hdist = (dist.iter().rposition(|&l| l > 0).unwrap_or(0) + 1).max(1);
    let all: Vec<u8> = litlen[..hlit].iter().chain(&dist[..hdist]).copied().collect();
    let mut runs = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let l = all[i];
        let mut run = all[i..].iter().take_while(|&&x| x == l).count();
        if l == 0 && run >= 3 {
            let r = run.min(138);
            runs.push(if r >= 11 { (18, (r - 11) as u8) } else { (17, (r - 3) as u8) });
            i += r;
            continue;
        }
        runs.push((l, 0));
        i += 1;
        run -= 1;
        if l != 0 {
            while run >= 3 {
                let r = run.min(6);
                runs.push((16, (r - 3) as u8));
                i += r;
                run -= r;
            }
        }
    }
    let mut cl_counts = [0u32; 19];
    for &(s, _) in &runs {
        cl_counts[s as usize] += 1;
    }
    let cl = code_lengths(&cl_counts, 7);
    let hclen = (CL_ORDER.iter().rposition(|&s| cl[s] > 0).unwrap_or(0) + 1).max(4);
    Header { litlen, dist, runs, cl, hlit, hdist, hclen }
}

fn extra_bits(s: u8) -> u32 {
    match s {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// The size in bits of a block with these symbol counts.
fn block_bits(c: &Counts) -> u64 {
    let h = header(c);
    let mut bits = 3 + 5 + 5 + 4 + 3 * h.hclen as u64;
    bits += h.runs.iter().map(|&(s, _)| h.cl[s as usize] as u64 + extra_bits(s) as u64).sum::<u64>();
    for (s, &n) in c.litlen.iter().enumerate() {
        let extra = if s > EOB { LEN_EXTRA[s - 257] as u64 } else { 0 };
        bits += n as u64 * (h.litlen[s] as u64 + extra);
    }
    for (s, &n) in c.dist.iter().enumerate() {
        bits += n as u64 * (h.dist[s] as u64 + DIST_EXTRA[s] as u64);
    }
    bits
}

fn write_block(out: &mut Bits, syms: &[Sym], last: bool) {
    let h = header(&counts(syms));
    let (lc, dc, cc) = (codes(&h.litlen), codes(&h.dist), codes(&h.cl));
    out.put(last as u32, 1);
    out.put(2, 2);
    out.put((h.hlit - 257) as u32, 5);
    out.put((h.hdist - 1) as u32, 5);
    out.put((h.hclen - 4) as u32, 4);
    for &s in &CL_ORDER[..h.hclen] {
        out.put(h.cl[s] as u32, 3);
    }
    for &(s, extra) in &h.runs {
        out.put(cc[s as usize] as u32, h.cl[s as usize] as u32);
        out.put(extra as u32, extra_bits(s));
    }
    for s in syms {
        if s.dist == 0 {
            out.put(lc[s.len as usize] as u32, h.litlen[s.len as usize] as u32);
        } else {
            let (len, dist) = (s.len as usize, s.dist as usize);
            let ls = len_symbol(len);
            out.put(lc[257 + ls] as u32, h.litlen[257 + ls] as u32);
            out.put((len - LEN_BASE[ls] as usize) as u32, LEN_EXTRA[ls] as u32);
            let ds = dist_symbol(dist);
            out.put(dc[ds] as u32, h.dist[ds] as u32);
            out.put((dist - DIST_BASE[ds] as usize) as u32, DIST_EXTRA[ds] as u32);
        }
    }
    out.put(lc[EOB] as u32, h.litlen[EOB] as u32);
}

/// Bits written least significant first.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, bits: u32) {
        self.acc |= (value as u64) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8]) -> usize {
        let z = zlib(data);
        assert_eq!(miniz_oxide::inflate::decompress_to_vec_zlib(&z).expect("inflates"), data);
        z.len()
    }

    /// Records like a map file's: repeated layouts with small differences.
    fn records() -> Vec<u8> {
        let mut out = Vec::new();
        let mut x = 12345u32;
        for i in 0..3000u32 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            out.extend(format!("c{:04}_{:04}\0", 1000 + (x >> 16) % 40 * 10, i % 97).encode_utf16().flat_map(u16::to_le_bytes));
            out.extend(((x >> 8) as f32 / 1000.0).to_le_bytes());
            out.extend([0u8; 12]);
            out.extend((i * 7 % 13).to_le_bytes());
        }
        out
    }

    #[test]
    fn inflates_back() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"abcabcabcabcabcabc");
        round_trip(&[0u8; 100_000]);
        let mut x = 1u32;
        let noise: Vec<u8> = (0..70_000)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                (x >> 24) as u8
            })
            .collect();
        round_trip(&noise);
        round_trip(&records());
    }

    #[test]
    fn smaller_than_miniz() {
        let data = records();
        let ours = round_trip(&data);
        let miniz = miniz_oxide::deflate::compress_to_vec_zlib(&data, 10).len();
        assert!(ours < miniz, "{ours} vs miniz {miniz}");
    }

    #[test]
    fn length_limits_hold() {
        // Fibonacci counts make a plain Huffman code deeper than 15 bits.
        let mut counts = vec![0u32; 30];
        let (mut a, mut b) = (1u32, 1u32);
        for c in counts.iter_mut() {
            *c = a;
            (a, b) = (b, a + b);
        }
        let lens = code_lengths(&counts, 15);
        assert!(lens.iter().all(|&l| (1..=15).contains(&l)));
        let kraft: f64 = lens.iter().map(|&l| 0.5f64.powi(l as i32)).sum();
        assert!((kraft - 1.0).abs() < 1e-9, "complete code");
    }
}
