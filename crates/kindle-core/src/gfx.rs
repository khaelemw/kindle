//! Scaleform movies (`.gfx`): read one into its tags, change some, write it back.
//!
//! A GFX file is an SWF with its own signature: `GFX` (or `FWS`), a version byte, the file
//! length (u32), the stage rect, frame rate and frame count, then tags. A tag header is a u16
//! (`code << 6 | length`), with a u32 length after it when the length field is 0x3F. Scaleform
//! adds its own tag codes (1000+: exporter info, external images) that are kept as they are.
//!
//! The game's menu movies (DS3 1.15.2) are uncompressed GFX v11. Their layout lives in
//! DefineSprite tags (nested tag lists) and PlaceObject2 tags (a child at a depth, with a
//! matrix and an instance name such as `Item_0_5`); the ActionScript only runs frame labels.
//! So a menu can be rearranged by editing placements and shapes alone.
//!
//! Each tag keeps its header form: the game's exporter writes some short tags with the long
//! header, and a movie read and written back unchanged is byte for byte the same.

/// A movie's tag codes used here.
pub const DEFINE_SHAPE: u16 = 2;
pub const PLACE_OBJECT2: u16 = 26;
pub const DEFINE_SPRITE: u16 = 39;
const SHAPE_CODES: [u16; 4] = [2, 22, 32, 83];
/// Tags that define a character id in their first two bytes.
const DEFINING_CODES: [u16; 6] = [2, 22, 32, 83, 39, 37];

/// Twips per movie unit (pixel).
pub const TWIPS: f64 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub code: u16,
    pub body: Vec<u8>,
    /// Written with the long header even if it would fit the short one.
    long: bool,
}

impl Tag {
    pub fn new(code: u16, body: Vec<u8>) -> Self {
        Self { code, body, long: false }
    }

    /// The character id this tag defines, if it defines one.
    pub fn defines(&self) -> Option<u16> {
        (DEFINING_CODES.contains(&self.code) && self.body.len() >= 2).then(|| u16::from_le_bytes([self.body[0], self.body[1]]))
    }

    pub fn is_shape(&self) -> bool {
        SHAPE_CODES.contains(&self.code)
    }

    fn write(&self, out: &mut Vec<u8>) {
        let len = self.body.len();
        if len < 0x3F && !self.long {
            out.extend_from_slice(&((self.code << 6) | len as u16).to_le_bytes());
        } else {
            out.extend_from_slice(&((self.code << 6) | 0x3F).to_le_bytes());
            out.extend_from_slice(&(len as u32).to_le_bytes());
        }
        out.extend_from_slice(&self.body);
    }
}

/// Tags from `data` until its end or an End tag (kept).
pub fn read_tags(data: &[u8]) -> Result<Vec<Tag>, String> {
    let mut tags = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let head = data.get(pos..pos + 2).ok_or("tag header past the end")?;
        let word = u16::from_le_bytes([head[0], head[1]]);
        pos += 2;
        let (code, mut len, long) = (word >> 6, (word & 0x3F) as usize, word & 0x3F == 0x3F);
        if long {
            let b = data.get(pos..pos + 4).ok_or("tag length past the end")?;
            len = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
            pos += 4;
        }
        let body = data.get(pos..pos + len).ok_or_else(|| format!("tag {code} longer than the movie"))?;
        pos += len;
        tags.push(Tag { code, body: body.to_vec(), long: long && len < 0x3F });
        if code == 0 {
            break;
        }
    }
    Ok(tags)
}

pub fn write_tags(tags: &[Tag]) -> Vec<u8> {
    let mut out = Vec::new();
    for t in tags {
        t.write(&mut out);
    }
    out
}

pub struct Movie {
    /// Signature through frame count.
    header: Vec<u8>,
    pub tags: Vec<Tag>,
}

impl Movie {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        match data.get(..3) {
            Some(b"GFX") | Some(b"FWS") => {}
            Some(b"CFX") | Some(b"CWS") => return Err("compressed movies aren't supported".into()),
            _ => return Err("not a GFX/SWF movie".into()),
        }
        let (_, end) = read_rect(data, 8).ok_or("movie header cut short")?;
        let start = end + 4;
        if data.len() < start {
            return Err("movie header cut short".into());
        }
        Ok(Self { header: data[..start].to_vec(), tags: read_tags(&data[start..])? })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.header.clone();
        out.extend(write_tags(&self.tags));
        let len = out.len() as u32;
        out[4..8].copy_from_slice(&len.to_le_bytes());
        out
    }

    /// Index of the tag defining character `id`.
    pub fn find(&self, id: u16) -> Option<usize> {
        self.tags.iter().position(|t| t.defines() == Some(id))
    }

    /// The highest character id defined.
    pub fn max_id(&self) -> u16 {
        self.tags.iter().filter_map(Tag::defines).max().unwrap_or(0)
    }

    /// The sprite defining `id`.
    pub fn sprite(&self, id: u16) -> Result<Sprite, String> {
        let tag = self.find(id).map(|i| &self.tags[i]).ok_or_else(|| format!("no character {id}"))?;
        if tag.code != DEFINE_SPRITE {
            return Err(format!("character {id} isn't a sprite"));
        }
        Sprite::parse(&tag.body)
    }

    pub fn set_sprite(&mut self, sprite: &Sprite) -> Result<(), String> {
        let i = self.find(sprite.id).ok_or_else(|| format!("no character {}", sprite.id))?;
        self.tags[i].body = sprite.to_body();
        Ok(())
    }

    /// The root timeline's placement named `name`.
    pub fn root_place(&self, name: &str) -> Option<Place> {
        self.tags.iter().filter(|t| t.code == PLACE_OBJECT2).filter_map(|t| Place::parse(&t.body).ok()).find(|p| p.name.as_deref() == Some(name))
    }

    /// Follows depth-1 children from `id` down to a shape (sprites that only wrap a shape).
    pub fn shape_under(&self, mut id: u16) -> Result<u16, String> {
        for _ in 0..8 {
            let tag = self.find(id).map(|i| &self.tags[i]).ok_or_else(|| format!("no character {id}"))?;
            if tag.is_shape() {
                return Ok(id);
            }
            let sprite = self.sprite(id)?;
            id = sprite
                .places()
                .find(|(_, p)| p.depth == 1 && p.id.is_some())
                .and_then(|(_, p)| p.id)
                .ok_or_else(|| format!("sprite {id} has nothing at depth 1"))?;
        }
        Err(format!("no shape under character {id}"))
    }
}

/// A DefineSprite: id, frame count and its own tags.
pub struct Sprite {
    pub id: u16,
    pub frames: u16,
    pub tags: Vec<Tag>,
}

impl Sprite {
    pub fn parse(body: &[u8]) -> Result<Self, String> {
        if body.len() < 4 {
            return Err("sprite cut short".into());
        }
        Ok(Self { id: u16::from_le_bytes([body[0], body[1]]), frames: u16::from_le_bytes([body[2], body[3]]), tags: read_tags(&body[4..])? })
    }

    pub fn to_body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.to_le_bytes());
        out.extend_from_slice(&self.frames.to_le_bytes());
        out.extend(write_tags(&self.tags));
        out
    }

    /// (tag index, placement) of each PlaceObject2.
    pub fn places(&self) -> impl Iterator<Item = (usize, Place)> + '_ {
        self.tags.iter().enumerate().filter(|(_, t)| t.code == PLACE_OBJECT2).filter_map(|(i, t)| Place::parse(&t.body).ok().map(|p| (i, p)))
    }

    /// (tag index, placement) of the child named `name`.
    pub fn place_named(&self, name: &str) -> Option<(usize, Place)> {
        self.places().find(|(_, p)| p.name.as_deref() == Some(name))
    }
}

/// The parts of a PlaceObject2 that menus are edited through.
#[derive(Debug)]
pub struct Place {
    pub depth: u16,
    pub id: Option<u16>,
    pub matrix: Option<Matrix>,
    pub name: Option<String>,
    /// Byte range of the matrix in the body.
    matrix_span: Option<(usize, usize)>,
}

impl Place {
    pub fn parse(body: &[u8]) -> Result<Self, String> {
        let flags = *body.first().ok_or("empty placement")?;
        let depth = u16::from_le_bytes([*body.get(1).ok_or("placement cut short")?, *body.get(2).ok_or("placement cut short")?]);
        let mut q = 3;
        let mut id = None;
        if flags & 2 != 0 {
            let b = body.get(q..q + 2).ok_or("placement cut short")?;
            id = Some(u16::from_le_bytes([b[0], b[1]]));
            q += 2;
        }
        let (mut matrix, mut matrix_span) = (None, None);
        if flags & 4 != 0 {
            let (m, end) = read_matrix(body, q).ok_or("placement matrix cut short")?;
            matrix = Some(m);
            matrix_span = Some((q, end));
            q = end;
        }
        if flags & 8 != 0 {
            q = skip_color_transform(body, q).ok_or("placement color transform cut short")?;
        }
        if flags & 0x10 != 0 {
            q += 2;
        }
        let mut name = None;
        if flags & 0x20 != 0 {
            let rest = body.get(q..).ok_or("placement name cut short")?;
            let end = rest.iter().position(|&c| c == 0).ok_or("placement name unterminated")?;
            name = Some(String::from_utf8_lossy(&rest[..end]).into_owned());
        }
        Ok(Self { depth, id, matrix, name, matrix_span })
    }

    /// A new placement of `id` at `depth` with `matrix`.
    pub fn new_body(depth: u16, id: u16, matrix: &Matrix) -> Vec<u8> {
        let mut out = vec![0x06];
        out.extend_from_slice(&depth.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend(write_matrix(matrix));
        out
    }

    /// A new placement of `id` at `depth` with `matrix` and the instance name `name`.
    pub fn new_named_body(depth: u16, id: u16, matrix: &Matrix, name: &str) -> Vec<u8> {
        let mut out = Self::new_body(depth, id, matrix);
        out[0] |= 0x20;
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        out
    }
}

/// The byte after a color transform (with alpha) at `pos`.
fn skip_color_transform(body: &[u8], pos: usize) -> Option<usize> {
    let mut b = Bits::new(body, pos);
    let (add, mult) = (b.u(1)?, b.u(1)?);
    let n = b.u(4)? as u32;
    for _ in 0..(4 * mult + 4 * add) {
        b.s(n)?;
    }
    Some(b.align())
}

/// `body` (a PlaceObject2 with a matrix) with the matrix replaced.
pub fn with_matrix(body: &[u8], matrix: &Matrix) -> Result<Vec<u8>, String> {
    let (s, e) = Place::parse(body)?.matrix_span.ok_or("placement has no matrix")?;
    let mut out = body[..s].to_vec();
    out.extend(write_matrix(matrix));
    out.extend_from_slice(&body[e..]);
    Ok(out)
}

/// `body` (a PlaceObject2) moved to `depth`.
pub fn with_depth(body: &[u8], depth: u16) -> Vec<u8> {
    let mut out = body.to_vec();
    out[1..3].copy_from_slice(&depth.to_le_bytes());
    out
}

/// A 2D transform: scale, rotate/skew, translation in movie units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub sx: f64,
    pub sy: f64,
    pub r0: f64,
    pub r1: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { sx: 1.0, sy: 1.0, r0: 0.0, r1: 0.0, tx: 0.0, ty: 0.0 };
}

pub struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8], byte: usize) -> Self {
        Self { data, pos: byte * 8 }
    }

    pub fn u(&mut self, n: u32) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..n {
            let byte = *self.data.get(self.pos >> 3)?;
            v = (v << 1) | ((byte >> (7 - (self.pos & 7))) & 1) as u64;
            self.pos += 1;
        }
        Some(v)
    }

    pub fn s(&mut self, n: u32) -> Option<i64> {
        let v = self.u(n)? as i64;
        Some(if n > 0 && v & (1 << (n - 1)) != 0 { v - (1 << n) } else { v })
    }

    /// Skips to the next byte and returns its index.
    pub fn align(&mut self) -> usize {
        self.pos = (self.pos + 7) & !7;
        self.pos >> 3
    }
}

#[derive(Default)]
pub struct BitWriter {
    bits: Vec<bool>,
}

impl BitWriter {
    pub fn u(&mut self, v: u64, n: u32) {
        self.bits.extend((0..n).map(|i| (v >> (n - 1 - i)) & 1 == 1));
    }

    pub fn s(&mut self, v: i64, n: u32) {
        self.u(v as u64 & ((1u64 << n) - 1), n);
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.bits.chunks(8).map(|c| c.iter().enumerate().fold(0u8, |b, (i, &bit)| b | ((bit as u8) << (7 - i)))).collect()
    }
}

/// Bits needed to hold every value as a signed number.
pub fn signed_bits(values: &[i64]) -> u32 {
    let mut n = 1;
    while !values.iter().all(|&v| -(1i64 << (n - 1)) <= v && v < (1i64 << (n - 1))) {
        n += 1;
    }
    n
}

/// A rect (xmin, xmax, ymin, ymax in twips) at byte `pos`, and the byte after it.
pub fn read_rect(data: &[u8], pos: usize) -> Option<([i64; 4], usize)> {
    let mut b = Bits::new(data, pos);
    let n = b.u(5)? as u32;
    let r = [b.s(n)?, b.s(n)?, b.s(n)?, b.s(n)?];
    Some((r, b.align()))
}

pub fn write_rect(r: [i64; 4]) -> Vec<u8> {
    let mut w = BitWriter::default();
    let n = signed_bits(&r);
    w.u(n as u64, 5);
    for v in r {
        w.s(v, n);
    }
    w.bytes()
}

pub fn read_matrix(data: &[u8], pos: usize) -> Option<(Matrix, usize)> {
    let mut b = Bits::new(data, pos);
    let mut m = Matrix::IDENTITY;
    if b.u(1)? == 1 {
        let n = b.u(5)? as u32;
        m.sx = b.s(n)? as f64 / 65536.0;
        m.sy = b.s(n)? as f64 / 65536.0;
    }
    if b.u(1)? == 1 {
        let n = b.u(5)? as u32;
        m.r0 = b.s(n)? as f64 / 65536.0;
        m.r1 = b.s(n)? as f64 / 65536.0;
    }
    let n = b.u(5)? as u32;
    m.tx = b.s(n)? as f64 / TWIPS;
    m.ty = b.s(n)? as f64 / TWIPS;
    Some((m, b.align()))
}

pub fn write_matrix(m: &Matrix) -> Vec<u8> {
    let mut w = BitWriter::default();
    for (has, a, b) in [((m.sx, m.sy) != (1.0, 1.0), m.sx, m.sy), ((m.r0, m.r1) != (0.0, 0.0), m.r0, m.r1)] {
        if has {
            let (a, b) = ((a * 65536.0).round() as i64, (b * 65536.0).round() as i64);
            let n = signed_bits(&[a, b]);
            w.u(1, 1);
            w.u(n as u64, 5);
            w.s(a, n);
            w.s(b, n);
        } else {
            w.u(0, 1);
        }
    }
    let (x, y) = ((m.tx * TWIPS).round() as i64, (m.ty * TWIPS).round() as i64);
    let n = if x != 0 || y != 0 { signed_bits(&[x, y]) } else { 0 };
    w.u(n as u64, 5);
    if n > 0 {
        w.s(x, n);
        w.s(y, n);
    }
    w.bytes()
}

/// A bitmap fill style: fill type (0x40-0x43), bitmap id, and the matrix from bitmap pixels to
/// the shape (scale 20 shows one pixel per unit).
pub type BitmapFill = (u8, u16, Matrix);

/// A shape's bounds and fill styles, for a DefineShape with bitmap fills only.
pub struct BitmapShape {
    /// Twips.
    pub bounds: [i64; 4],
    pub fills: Vec<BitmapFill>,
}

/// The bounds and fills of a DefineShape (code 2) body whose fills are all bitmaps.
pub fn bitmap_shape(body: &[u8]) -> Result<BitmapShape, String> {
    let (bounds, mut q) = read_rect(body, 2).ok_or("shape cut short")?;
    let count = *body.get(q).ok_or("shape cut short")?;
    q += 1;
    let mut fills = Vec::new();
    for _ in 0..count {
        let kind = *body.get(q).ok_or("shape cut short")?;
        if !(0x40..=0x43).contains(&kind) {
            return Err(format!("fill type {kind:#x} isn't a bitmap"));
        }
        let bitmap = u16::from_le_bytes([*body.get(q + 1).ok_or("shape cut short")?, *body.get(q + 2).ok_or("shape cut short")?]);
        let (m, end) = read_matrix(body, q + 3).ok_or("shape cut short")?;
        fills.push((kind, bitmap, m));
        q = end;
    }
    Ok(BitmapShape { bounds, fills })
}

/// A DefineShape (code 2) body: rectangles in twips `(x0, x1, y0, y1)`, each filled with its
/// own bitmap fill style `(fill type, bitmap id, matrix)`. Drawn like the game's exporter
/// draws a rectangle: move to the bottom right, fill on the right side, then left, up, right,
/// down.
pub fn rect_shape(id: u16, bounds: [i64; 4], rects: &[([i64; 4], BitmapFill)]) -> Vec<u8> {
    let mut out = id.to_le_bytes().to_vec();
    out.extend(write_rect(bounds));
    out.push(rects.len() as u8);
    for (_, (kind, bitmap, m)) in rects {
        out.push(*kind);
        out.extend_from_slice(&bitmap.to_le_bytes());
        out.extend(write_matrix(m));
    }
    out.push(0); // no line styles
    let fill_bits = (32 - (rects.len() as u32).leading_zeros()).max(1);
    out.push((fill_bits << 4) as u8);
    let mut w = BitWriter::default();
    for (i, ([x0, x1, y0, y1], _)) in rects.iter().enumerate() {
        let n = signed_bits(&[*x1, *y1]);
        w.u(0, 1);
        w.u(0b00101, 5); // fill style 1 and move to
        w.u(n as u64, 5);
        w.s(*x1, n);
        w.s(*y1, n);
        w.u(i as u64 + 1, fill_bits);
        for (vertical, delta) in [(false, x0 - x1), (true, y0 - y1), (false, x1 - x0), (true, y1 - y0)] {
            let n = signed_bits(&[delta]).max(2);
            w.u(0b11, 2); // straight edge
            w.u(n as u64 - 2, 4);
            w.u(0, 1);
            w.u(vertical as u64, 1);
            w.s(delta, n);
        }
    }
    w.u(0, 6);
    out.extend(w.bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW_FRAME: u16 = 1;

    fn movie_bytes(tags: &[Tag]) -> Vec<u8> {
        let mut out = b"GFX\x0b\0\0\0\0".to_vec();
        out.extend(write_rect([0, 38400, 0, 21600]));
        out.extend_from_slice(&[0, 30, 1, 0]);
        out.extend(write_tags(tags));
        let len = out.len() as u32;
        out[4..8].copy_from_slice(&len.to_le_bytes());
        out
    }

    #[test]
    fn matrices_round_trip() {
        for m in [
            Matrix::IDENTITY,
            Matrix { sx: 1.0699920654296875, ..Matrix::IDENTITY },
            Matrix { sx: 20.0, sy: 20.0, r0: 0.0, r1: 0.0, tx: -304.0, ty: -372.0 },
            Matrix { sx: -1.0, sy: 1.0, r0: 0.5, r1: -0.25, tx: 296.0, ty: -100.0 },
        ] {
            let bytes = write_matrix(&m);
            assert_eq!(read_matrix(&bytes, 0).unwrap(), (m, bytes.len()));
        }
    }

    #[test]
    fn movies_round_trip_with_header_forms() {
        let place = Place::new_body(3, 7, &Matrix { tx: 12.5, ..Matrix::IDENTITY });
        let mut long = Tag::new(PLACE_OBJECT2, place.clone());
        long.long = true;
        let sprite = Sprite { id: 9, frames: 1, tags: vec![long, Tag::new(SHOW_FRAME, vec![]), Tag::new(0, vec![])] };
        let tags = vec![Tag::new(DEFINE_SPRITE, sprite.to_body()), Tag::new(SHOW_FRAME, vec![]), Tag::new(0, vec![])];
        let bytes = movie_bytes(&tags);
        let movie = Movie::parse(&bytes).unwrap();
        assert_eq!(movie.to_bytes(), bytes);
        let s = movie.sprite(9).unwrap();
        assert!(s.tags[0].long);
        let (_, p) = s.places().next().unwrap();
        assert_eq!((p.depth, p.id, p.matrix.unwrap().tx), (3, Some(7), 12.5));
    }

    #[test]
    fn placements_edit_in_place() {
        let mut body = Place::new_body(1, 50, &Matrix { sx: 0.5625, sy: 0.5625, ..Matrix::IDENTITY });
        body[0] |= 0x20;
        body.extend_from_slice(b"Cursor\0");
        let moved = with_depth(&with_matrix(&body, &Matrix { tx: -245.0, ty: -100.0, ..Matrix::IDENTITY }).unwrap(), 3);
        let p = Place::parse(&moved).unwrap();
        assert_eq!((p.depth, p.id, p.name.as_deref()), (3, Some(50), Some("Cursor")));
        assert_eq!(p.matrix.unwrap(), Matrix { tx: -245.0, ty: -100.0, ..Matrix::IDENTITY });
    }

    #[test]
    fn rect_shapes_read_back() {
        let fill = (0x40, 1, Matrix { sx: 20.0, sy: 20.0, r0: 0.0, r1: 0.0, tx: -304.0, ty: -372.0 });
        let flipped = (0x40, 1, Matrix { sx: -20.4, ..fill.2 });
        let body = rect_shape(21, [-6980, 6980, -4000, 4000], &[([-6980, 6980, -4000, -260], fill), ([-6200, -5955, -260, 2340], flipped)]);
        let shape = bitmap_shape(&body).unwrap();
        assert_eq!(shape.bounds, [-6980, 6980, -4000, 4000]);
        assert_eq!(shape.fills.len(), 2);
        assert_eq!(shape.fills[0], fill);
        assert!((shape.fills[1].2.sx + 20.4).abs() < 1e-4);
    }
}
