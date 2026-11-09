//! Mods' drawing on the game's screen (kindle.ui.layer): layers of text, boxes, lines, bars,
//! circles and pictures (PNG files in the mod's folder), placed on a 1920 x 1080 canvas scaled to the window, or pinned to a point in the world
//! (a marker over a bonfire, a health bar over an enemy) and drawn where the camera sees it.
//!
//! A layer is kept until the mod changes or removes it, so a mod sets it when its numbers change,
//! not every frame (it may every frame, for something that moves). The mod's Lua runs on the
//! game's thread and the layers are drawn on the render thread: they share the layers here, and
//! the camera, which the game's frame reads for the render thread while a layer has world points.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mlua::{Table, Value};

use crate::lock;

/// The canvas a layer's x and y are on, scaled to the window (the game's own UI is laid out
/// for 1080p the same way).
pub const CANVAS: [f32; 2] = [1920.0, 1080.0];
/// The most elements a layer holds.
pub const MAX_ELEMENTS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Font {
    /// The game-like serif (Garamond, else Georgia or Times).
    Serif,
    /// The overlay's sans (Segoe UI).
    Sans,
}

/// A picture a layer shows, decoded once (RGBA, 8 bits a channel); the renderer makes its
/// texture the first time it draws it.
pub struct Image {
    /// Unique per file and version: the renderer's key for its texture.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image {} ({} x {})", self.id, self.width, self.height)
    }
}

/// Finds a picture by the path a mod gave (in its folder).
pub type Images<'a> = dyn Fn(&str) -> Result<Arc<Image>, String> + 'a;

/// Every picture loaded, for the renderer to make the textures of.
pub fn images() -> Vec<Arc<Image>> {
    lock(&IMAGES).iter().map(|(_, _, image)| image.clone()).collect()
}

/// The largest picture, per side, and how many different pictures (files, and versions of a file
/// changed while the game runs) Kindle keeps: their textures stay until the game closes.
pub const MAX_IMAGE_SIDE: u32 = 4096;
pub const MAX_IMAGES: usize = 256;

/// Pictures by file, with the file's modified time they were read at.
type ImageCache = Vec<(PathBuf, std::time::SystemTime, Arc<Image>)>;
static IMAGES: Mutex<ImageCache> = Mutex::new(Vec::new());

/// A PNG file's picture: read and decoded the first time, then the same one until the file changes.
pub fn load_image(path: &Path) -> Result<Arc<Image>, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).map_err(|e| e.to_string())?;
    cached_image(path, modified, || decode_png(&std::fs::read(path).map_err(|e| e.to_string())?))
}

/// A picture Kindle makes once by a key of its own (the game's icons: `game:icon/<id>`), kept
/// with the files' pictures: `make` runs only the first time.
pub fn cached_image(key: &Path, version: std::time::SystemTime, make: impl FnOnce() -> Result<(u32, u32, Vec<u8>), String>) -> Result<Arc<Image>, String> {
    let mut images = lock(&IMAGES);
    if let Some((_, _, image)) = images.iter().find(|(p, m, _)| p == key && *m == version) {
        return Ok(image.clone());
    }
    if images.len() >= MAX_IMAGES {
        return Err(format!("more than {MAX_IMAGES} pictures loaded"));
    }
    let (width, height, rgba) = make()?;
    let image = Arc::new(Image { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), width, height, rgba });
    images.push((key.to_path_buf(), version, image.clone()));
    Ok(image)
}

/// A PNG as RGBA, 8 bits a channel (any PNG's colours: grey, palette, 16-bit).
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| format!("not a PNG the decoder reads: {e}"))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        return Err(format!("{width} x {height}: pictures are 1 to {MAX_IMAGE_SIDE} pixels a side"));
    }
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too big")?];
    let frame = reader.next_frame(&mut buf).map_err(|e| format!("not a PNG the decoder reads: {e}"))?;
    let pixels = &buf[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("a palette PNG the decoder didn't expand".into()),
    };
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("the PNG's pixels don't fill its size".into());
    }
    Ok((width, height, rgba))
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Text {
        text: String,
        size: f32,
        font: Font,
        align: Align,
        shadow: bool,
    },
    Rect {
        w: f32,
        h: f32,
        filled: bool,
        rounding: f32,
        thickness: f32,
    },
    Line {
        dx: f32,
        dy: f32,
        thickness: f32,
    },
    /// A bar filled `value` (0-1) of its width with the element's colour, over `back`.
    Bar {
        w: f32,
        h: f32,
        value: f32,
        back: [f32; 4],
    },
    Circle {
        r: f32,
        filled: bool,
        thickness: f32,
    },
    /// A picture stretched to w x h, tinted by the element's colour (white: as it is).
    Image {
        image: Arc<Image>,
        w: f32,
        h: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// A point in the world the element is drawn at (x and y are then an offset from it on the
    /// canvas), or None: x and y are on the canvas.
    pub at: Option<[f32; 3]>,
    /// Hidden when its world point is further than this from the camera (metres).
    pub range: Option<f32>,
    pub x: f32,
    pub y: f32,
    pub color: [f32; 4],
    pub shape: Shape,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub owner: String,
    pub id: u64,
    pub elements: Vec<Element>,
    pub visible: bool,
}

/// Where the camera is and how it's turned: unit vectors to its right, up and where it looks,
/// and its vertical field of view (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pos: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub fov: f32,
}

static LAYERS: Mutex<Vec<Layer>> = Mutex::new(Vec::new());
static CAMERA: Mutex<Option<Camera>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Adds a layer of `owner`'s on top of the others; returns its id.
pub fn add(owner: &str, elements: Vec<Element>) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lock(&LAYERS).push(Layer { owner: owner.into(), id, elements, visible: true });
    id
}

fn with_layer<T>(owner: &str, id: u64, f: impl FnOnce(&mut Layer) -> T) -> Result<T, String> {
    let mut layers = lock(&LAYERS);
    let layer = layers.iter_mut().find(|l| l.id == id && l.owner == owner).ok_or("that layer was removed")?;
    Ok(f(layer))
}

/// Replaces what a layer draws.
pub fn set(owner: &str, id: u64, elements: Vec<Element>) -> Result<(), String> {
    with_layer(owner, id, |l| l.elements = elements)
}

pub fn show(owner: &str, id: u64, visible: bool) -> Result<(), String> {
    with_layer(owner, id, |l| l.visible = visible)
}

pub fn remove(owner: &str, id: u64) -> bool {
    let mut layers = lock(&LAYERS);
    let before = layers.len();
    layers.retain(|l| !(l.id == id && l.owner == owner));
    layers.len() != before
}

/// A mod unloaded or stopped: its layers go.
pub fn remove_owner(owner: &str) {
    lock(&LAYERS).retain(|l| l.owner != owner);
}

/// The layers shown, bottom first, for the render thread.
pub fn shown() -> Vec<Layer> {
    lock(&LAYERS).iter().filter(|l| l.visible && !l.elements.is_empty()).cloned().collect()
}

/// Whether a shown layer has a world point, so the game's frame should pass on the camera.
pub fn wants_camera() -> bool {
    lock(&LAYERS).iter().any(|l| l.visible && l.elements.iter().any(|e| e.at.is_some()))
}

/// From the game's frame: the camera this frame (None: there's none, world points hide).
pub fn set_camera(camera: Option<Camera>) {
    *lock(&CAMERA) = camera;
}

pub fn camera() -> Option<Camera> {
    *lock(&CAMERA)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Where a world point shows in a window `size` pixels big, and how far it is from the camera;
/// None behind the camera.
pub fn project(cam: &Camera, point: [f32; 3], size: [f32; 2]) -> Option<([f32; 2], f32)> {
    let v = [point[0] - cam.pos[0], point[1] - cam.pos[1], point[2] - cam.pos[2]];
    let depth = dot(v, cam.forward);
    if depth < 0.05 {
        return None;
    }
    let focal = (size[1] / 2.0) / (cam.fov / 2.0).tan();
    let x = size[0] / 2.0 + dot(v, cam.right) / depth * focal;
    let y = size[1] / 2.0 - dot(v, cam.up) / depth * focal;
    let distance = dot(v, v).sqrt();
    (x.is_finite() && y.is_finite()).then_some(([x, y], distance))
}

/// The canvas's scale and origin in a window `size` pixels big: uniform, centred (a wider
/// window keeps the canvas's 16:9 in its middle).
pub fn canvas_fit(size: [f32; 2]) -> (f32, [f32; 2]) {
    let s = (size[0] / CANVAS[0]).min(size[1] / CANVAS[1]);
    (s, [(size[0] - CANVAS[0] * s) / 2.0, (size[1] - CANVAS[1] * s) / 2.0])
}

// ── from Lua ────────────────────────────────────────────────────────────────

fn color_from(v: Value, default: [f32; 4]) -> Result<[f32; 4], String> {
    match v {
        Value::Nil => Ok(default),
        Value::String(s) => {
            let s = s.to_str().map_err(|e| e.to_string())?.to_string();
            let hex = s.trim_start_matches('#');
            let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or(""), 16).map(|b| b as f32 / 255.0);
            match hex.len() {
                6 | 8 => {
                    let a = if hex.len() == 8 { byte(6) } else { Ok(1.0) };
                    match (byte(0), byte(2), byte(4), a) {
                        (Ok(r), Ok(g), Ok(b), Ok(a)) => Ok([r, g, b, a]),
                        _ => Err(format!("color {s:?} isn't \"#rrggbb\" or \"#rrggbbaa\"")),
                    }
                }
                _ => Err(format!("color {s:?} isn't \"#rrggbb\" or \"#rrggbbaa\"")),
            }
        }
        Value::Table(t) => {
            let c = |i: usize, d: f32| t.get::<Option<f32>>(i).ok().flatten().unwrap_or(d).clamp(0.0, 1.0);
            Ok([c(1, 1.0), c(2, 1.0), c(3, 1.0), c(4, 1.0)])
        }
        _ => Err("color must be \"#rrggbb\" or {r, g, b, a} (0-1)".into()),
    }
}

/// The text colour the game's menus use.
const TEXT: [f32; 4] = [0.92, 0.89, 0.82, 1.0];

fn num(t: &Table, k: &str) -> Result<Option<f32>, String> {
    match t.get::<Value>(k).unwrap_or(Value::Nil) {
        Value::Nil => Ok(None),
        Value::Integer(i) => Ok(Some(i as f32)),
        Value::Number(f) if f.is_finite() => Ok(Some(f as f32)),
        _ => Err(format!("{k} must be a number")),
    }
}

fn point_from(v: Value) -> Result<Option<[f32; 3]>, String> {
    let Value::Table(t) = v else {
        return match v {
            Value::Nil => Ok(None),
            _ => Err("at must be {x, y, z} or a table with x, y and z (a character)".into()),
        };
    };
    let named = (num(&t, "x")?, num(&t, "y")?, num(&t, "z")?);
    if let (Some(x), Some(y), Some(z)) = named {
        return Ok(Some([x, y, z]));
    }
    let i = |n: usize| t.get::<Option<f32>>(n).ok().flatten();
    match (i(1), i(2), i(3)) {
        (Some(x), Some(y), Some(z)) => Ok(Some([x, y, z])),
        _ => Err("at must be {x, y, z} or a table with x, y and z (a character)".into()),
    }
}

/// An element: {x, y, color, at?, range?} and one of text = "...", rect = {w, h}, line = {dx,
/// dy}, bar = {w, h, value}, circle = {r}, image = "file.png" (w and h: its own size if not given).
/// `images` finds a picture by the path the mod gave.
fn element_from(t: &Table, images: &Images<'_>) -> Result<Element, String> {
    let at = point_from(t.get("at").unwrap_or(Value::Nil))?;
    let range = num(t, "range")?;
    let x = num(t, "x")?.unwrap_or(0.0);
    let y = num(t, "y")?.unwrap_or(0.0);
    let image = t.get::<Option<String>>("image").map_err(|_| "image must be a file's path in the mod's folder or the game's icon id")?;
    let color = color_from(t.get("color").unwrap_or(Value::Nil), if image.is_some() { [1.0; 4] } else { TEXT })?;
    let flag = |k: &str, d: bool| t.get::<Option<bool>>(k).ok().flatten().unwrap_or(d);
    let thickness = num(t, "thickness")?.unwrap_or(2.0).max(0.5);
    let shape = if let Some(path) = image {
        let image = images(&path).map_err(|e| format!("image {path:?}: {e}"))?;
        let w = num(t, "w")?.unwrap_or(image.width as f32).max(0.0);
        let h = num(t, "h")?.unwrap_or(image.height as f32).max(0.0);
        Shape::Image { image, w, h }
    } else if let Some(text) = t.get::<Option<String>>("text").map_err(|_| "text must be text")? {
        let align = match t.get::<Option<String>>("align").ok().flatten().as_deref() {
            None | Some("left") => Align::Left,
            Some("center") | Some("centre") => Align::Center,
            Some("right") => Align::Right,
            Some(a) => return Err(format!("align {a:?}: \"left\", \"center\" or \"right\"")),
        };
        let font = match t.get::<Option<String>>("font").ok().flatten().as_deref() {
            None | Some("serif") => Font::Serif,
            Some("sans") => Font::Sans,
            Some(f) => return Err(format!("font {f:?}: \"serif\" or \"sans\"")),
        };
        Shape::Text { text, size: num(t, "size")?.unwrap_or(28.0).clamp(6.0, 200.0), font, align, shadow: flag("shadow", true) }
    } else if let Ok(Some(r)) = t.get::<Option<Table>>("rect") {
        let w = num(&r, "w")?.or(r.get::<Option<f32>>(1).ok().flatten()).ok_or("rect needs {w, h}")?;
        let h = num(&r, "h")?.or(r.get::<Option<f32>>(2).ok().flatten()).ok_or("rect needs {w, h}")?;
        Shape::Rect { w, h, filled: flag("filled", true), rounding: num(t, "rounding")?.unwrap_or(0.0).max(0.0), thickness }
    } else if let Ok(Some(l)) = t.get::<Option<Table>>("line") {
        let dx = num(&l, "dx")?.or(l.get::<Option<f32>>(1).ok().flatten()).ok_or("line needs {dx, dy}")?;
        let dy = num(&l, "dy")?.or(l.get::<Option<f32>>(2).ok().flatten()).ok_or("line needs {dx, dy}")?;
        Shape::Line { dx, dy, thickness }
    } else if let Ok(Some(b)) = t.get::<Option<Table>>("bar") {
        let w = num(&b, "w")?.or(b.get::<Option<f32>>(1).ok().flatten()).ok_or("bar needs {w, h, value}")?;
        let h = num(&b, "h")?.or(b.get::<Option<f32>>(2).ok().flatten()).ok_or("bar needs {w, h, value}")?;
        let value = num(&b, "value")?.or(b.get::<Option<f32>>(3).ok().flatten()).unwrap_or(0.0);
        let back = color_from(t.get("back").unwrap_or(Value::Nil), [0.05, 0.04, 0.03, 0.75])?;
        Shape::Bar { w, h, value: if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 }, back }
    } else if let Ok(Some(c)) = t.get::<Option<Table>>("circle") {
        let r = num(&c, "r")?.or(c.get::<Option<f32>>(1).ok().flatten()).ok_or("circle needs {r}")?;
        Shape::Circle { r: r.max(0.5), filled: flag("filled", true), thickness }
    } else {
        return Err("it needs one of text, rect, line, bar, circle or image".into());
    };
    Ok(Element { at, range, x, y, color, shape })
}

/// A layer's elements from Lua: a list of element tables. `images` finds a picture by the path
/// the mod gave (in its folder).
pub fn from_lua(list: &Table, images: &Images<'_>) -> Result<Vec<Element>, String> {
    let mut out = Vec::new();
    for (i, e) in list.sequence_values::<Table>().enumerate() {
        let n = i + 1;
        let e = e.map_err(|_| format!("element {n} isn't a table"))?;
        if out.len() >= MAX_ELEMENTS {
            return Err(format!("more than {MAX_ELEMENTS} elements"));
        }
        out.push(element_from(&e, images).map_err(|e| format!("element {n}: {e}"))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_belong_to_their_mod() {
        let a = add("test-hud-a", Vec::new());
        assert!(set("test-hud-b", a, Vec::new()).is_err());
        assert!(show("test-hud-a", a, false).is_ok());
        assert!(!remove("test-hud-b", a));
        let b = add("test-hud-a", Vec::new());
        remove_owner("test-hud-a");
        assert!(set("test-hud-a", a, Vec::new()).is_err() && set("test-hud-a", b, Vec::new()).is_err());
    }

    #[test]
    fn points_project_through_the_camera() {
        // Looking down +z from the origin, 90 degrees tall, in a 1000 x 500 window.
        let cam = Camera { pos: [0.0; 3], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], forward: [0.0, 0.0, 1.0], fov: std::f32::consts::FRAC_PI_2 };
        let ([x, y], d) = project(&cam, [0.0, 0.0, 10.0], [1000.0, 500.0]).unwrap();
        assert!((x - 500.0).abs() < 1e-3 && (y - 250.0).abs() < 1e-3 && (d - 10.0).abs() < 1e-3);
        // At the top edge of the view: as high as it is far.
        let ([_, y], _) = project(&cam, [0.0, 10.0, 10.0], [1000.0, 500.0]).unwrap();
        assert!(y.abs() < 1e-3);
        let ([x, _], _) = project(&cam, [5.0, 0.0, 10.0], [1000.0, 500.0]).unwrap();
        assert!((x - 625.0).abs() < 1e-3);
        assert!(project(&cam, [0.0, 0.0, -1.0], [1000.0, 500.0]).is_none());
        assert_eq!(canvas_fit([3840.0, 1080.0]), (1.0, [960.0, 0.0]));
    }

    fn no_images(_: &str) -> Result<Arc<Image>, String> {
        Err("no pictures here".into())
    }

    #[test]
    fn pngs_decode_to_rgba() {
        // A 2 x 1 grey PNG (8-bit grayscale), made with a stored deflate block.
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let chunk = |png: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            png.extend((data.len() as u32).to_be_bytes());
            let start = png.len();
            png.extend(kind);
            png.extend(data);
            let crc = crc32(&png[start..]);
            png.extend(crc.to_be_bytes());
        };
        chunk(&mut png, b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 1, 8, 0, 0, 0, 0]);
        let raw = [0u8, 10, 200]; // filter none, then two pixels
        let adler = {
            let (mut a, mut b) = (1u32, 0u32);
            for &x in &raw {
                a = (a + x as u32) % 65521;
                b = (b + a) % 65521;
            }
            (b << 16) | a
        };
        let mut z = vec![0x78, 0x01, 0x01, 3, 0, 0xFC, 0xFF];
        z.extend(raw);
        z.extend(adler.to_be_bytes());
        chunk(&mut png, b"IDAT", &z);
        chunk(&mut png, b"IEND", &[]);
        let (w, h, rgba) = decode_png(&png).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, [10, 10, 10, 255, 200, 200, 200, 255]);
        assert!(decode_png(b"not a png").is_err());
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in bytes {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    #[test]
    fn elements_from_lua() {
        let lua = mlua::Lua::new();
        let t: Table = lua
            .load(
                r##"return {
                    { text = "Hello", x = 100, y = 50, color = "#ff8000", align = "center" },
                    { rect = { w = 200, h = 20 }, x = 10, y = 10, color = { 0, 0, 0, 0.5 } },
                    { bar = { 200, 8, 0.25 }, x = -100, y = -40, at = { x = 1, y = 2, z = 3 }, range = 30 },
                    { line = { 10, 0 } },
                    { circle = { r = 4 }, filled = false, at = { 4, 5, 6 } },
                }"##,
            )
            .eval()
            .unwrap();
        let e = from_lua(&t, &no_images).unwrap();
        assert_eq!(e.len(), 5);
        assert_eq!(e[0].color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
        assert!(matches!(&e[0].shape, Shape::Text { text, align: Align::Center, size, .. } if text == "Hello" && *size == 28.0));
        assert_eq!(e[1].color, [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(e[2].at, Some([1.0, 2.0, 3.0]));
        assert_eq!(e[2].range, Some(30.0));
        assert!(matches!(e[2].shape, Shape::Bar { value, .. } if value == 0.25));
        assert!(matches!(e[4].shape, Shape::Circle { filled: false, .. }));
        let bad: Table = lua.load(r#"return { { x = 1 } }"#).eval().unwrap();
        assert!(from_lua(&bad, &no_images).unwrap_err().contains("element 1"));
        let pic = Arc::new(Image { id: 0, width: 16, height: 8, rgba: vec![0; 16 * 8 * 4] });
        let t: Table = lua.load(r##"return { { image = "icon.png", x = 5 }, { image = "icon.png", w = 32, color = "#ff000080" } }"##).eval().unwrap();
        let e = from_lua(&t, &|p: &str| if p == "icon.png" { Ok(pic.clone()) } else { Err("missing".into()) }).unwrap();
        assert!(matches!(&e[0].shape, Shape::Image { w, h, .. } if *w == 16.0 && *h == 8.0));
        assert_eq!(e[0].color, [1.0; 4]);
        assert!(matches!(&e[1].shape, Shape::Image { w, h, .. } if *w == 32.0 && *h == 8.0));
        let bad: Table = lua.load(r#"return { { image = "gone.png" } }"#).eval().unwrap();
        assert!(from_lua(&bad, &no_images).unwrap_err().contains("gone.png"));
        let bad: Table = lua.load(r#"return { { text = "x", color = "red" } }"#).eval().unwrap();
        assert!(from_lua(&bad, &no_images).unwrap_err().contains("color"));
    }
}
