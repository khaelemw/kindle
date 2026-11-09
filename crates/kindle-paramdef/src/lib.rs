//! Parses FromSoftware `PARAMDEF` XML files (the format used by Paramdex / Smithbox /
//! fromsoftware-rs) and computes the byte layout of each field, so param rows can be
//! read and written by field name at runtime.
//!
//! Layout rules follow SoulsFormats: fields are packed in declaration order with no
//! implicit padding (paramdefs declare their own `dummy8` padding), and consecutive
//! bitfields share a storage unit of their base type until it is full.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
    F32,
    /// Padding / unknown bytes. Exposed as raw unsigned bytes.
    Dummy8,
    /// Fixed-length single-byte string (Shift-JIS in practice; read as lossy ASCII).
    FixStr,
    /// Fixed-length UTF-16 string.
    FixStrW,
}

impl FieldType {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "u8" => Self::U8,
            "s8" => Self::S8,
            "u16" => Self::U16,
            "s16" => Self::S16,
            "u32" => Self::U32,
            "s32" => Self::S32,
            "f32" => Self::F32,
            "dummy8" => Self::Dummy8,
            "fixstr" => Self::FixStr,
            "fixstrW" => Self::FixStrW,
            _ => return None,
        })
    }

    /// Size in bytes of one element.
    pub fn size(self) -> usize {
        match self {
            Self::U8 | Self::S8 | Self::Dummy8 | Self::FixStr => 1,
            Self::U16 | Self::S16 | Self::FixStrW => 2,
            Self::U32 | Self::S32 | Self::F32 => 4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::S8 => "s8",
            Self::U16 => "u16",
            Self::S16 => "s16",
            Self::U32 => "u32",
            Self::S32 => "s32",
            Self::F32 => "f32",
            Self::Dummy8 => "dummy8",
            Self::FixStr => "fixstr",
            Self::FixStrW => "fixstrW",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
    /// Byte offset from the start of the row.
    pub offset: usize,
    /// Element count (1 unless declared as `name[N]`).
    pub count: usize,
    /// For bitfields: (bit offset within the storage unit, bit width).
    pub bits: Option<(u8, u8)>,
    /// The values the game's own param editor allows (`<Minimum>`, `<Maximum>`), when the
    /// paramdef gives them and they fit the field's type (a wrapped one, a negative limit on a
    /// field shown unsigned, is left out). See `ParamDef::check`.
    pub limits: Option<(f64, f64)>,
}

impl FieldType {
    /// The smallest and largest value the type holds (None for strings).
    pub fn bounds(self) -> Option<(f64, f64)> {
        Some(match self {
            Self::U8 | Self::Dummy8 => (0.0, u8::MAX as f64),
            Self::S8 => (i8::MIN as f64, i8::MAX as f64),
            Self::U16 => (0.0, u16::MAX as f64),
            Self::S16 => (i16::MIN as f64, i16::MAX as f64),
            Self::U32 => (0.0, u32::MAX as f64),
            Self::S32 => (i32::MIN as f64, i32::MAX as f64),
            Self::F32 => (f32::MIN as f64, f32::MAX as f64),
            Self::FixStr | Self::FixStrW => return None,
        })
    }
}

/// A field's `<Minimum>` and `<Maximum>`, the type's own bounds standing in for a missing one;
/// None when neither is given or they don't fit the type (min > max, or outside its bounds), or
/// when a field shown unsigned (an unsigned type, or `<DisplayFormat>` %u) has a negative one: a
/// limit past the signed range stored wrapped. DS3's ActionButtonParam keeps 4,000,000,000 as
/// -294967297 on its s32 flags (invalidFlag, grayoutFlag), whose own rows mostly hold -1 (no
/// flag): read as given, no positive flag fitted, read as u32, the game's -1 wouldn't.
fn limits_of(node: &roxmltree::Node, ty: FieldType) -> Option<(f64, f64)> {
    let text = |tag: &str| node.children().find(|n| n.has_tag_name(tag)).and_then(|n| n.text()).map(str::trim);
    let num = |tag: &str| text(tag).and_then(|t| t.parse::<f64>().ok());
    let (lo, hi) = (num("Minimum"), num("Maximum"));
    if lo.is_none() && hi.is_none() {
        return None;
    }
    let shown_unsigned =
        matches!(ty, FieldType::U8 | FieldType::U16 | FieldType::U32 | FieldType::Dummy8) || text("DisplayFormat").is_some_and(|f| f.contains("%u"));
    if shown_unsigned && (lo.is_some_and(|v| v < 0.0) || hi.is_some_and(|v| v < 0.0)) {
        return None;
    }
    let (tlo, thi) = ty.bounds()?;
    let (lo, hi) = (lo.unwrap_or(tlo), hi.unwrap_or(thi));
    (lo <= hi && lo >= tlo && hi <= thi).then_some((lo, hi))
}

impl Field {
    /// Whether this field is a plain number that can be read and written as one value.
    pub fn is_scalar(&self) -> bool {
        self.count == 1 && !matches!(self.ty, FieldType::FixStr | FieldType::FixStrW)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f32),
    Str(String),
    Bytes(Vec<u8>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Bytes(b) => write!(f, "{b:02x?}"),
        }
    }
}

#[derive(Debug)]
pub struct ParamDef {
    /// The definition type name, e.g. `EQUIP_PARAM_GOODS_ST`.
    pub param_type: String,
    pub fields: Vec<Field>,
    /// Total row size in bytes.
    pub size: usize,
    by_name: HashMap<String, usize>,
    by_lower_name: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

macro_rules! err {
    ($($arg:tt)*) => { Error(format!($($arg)*)) };
}

/// One parsed `Def="..."` attribute, before layout.
struct RawField {
    ty: FieldType,
    name: String,
    count: usize,
    bit_width: Option<u8>,
}

fn parse_def(def: &str) -> Result<RawField, Error> {
    // Grammar: `<type> <name>[\[N\]][:bits][ = default]`
    let def = def.split('=').next().unwrap_or(def).trim();
    let (ty_str, rest) = def.split_once(char::is_whitespace).ok_or_else(|| err!("malformed field def {def:?}"))?;
    let ty = FieldType::parse(ty_str).ok_or_else(|| err!("unknown field type {ty_str:?} in {def:?}"))?;
    let rest = rest.trim();

    let (rest, bit_width) = match rest.split_once(':') {
        Some((n, b)) => {
            let w: u8 = b.trim().parse().map_err(|_| err!("bad bit width in {def:?}"))?;
            (n.trim(), Some(w))
        }
        None => (rest, None),
    };

    let (name, count) = match rest.split_once('[') {
        Some((n, c)) => {
            let c = c.trim_end_matches(']');
            let c: usize = c.parse().map_err(|_| err!("bad array length in {def:?}"))?;
            (n.trim(), c)
        }
        None => (rest, 1),
    };

    Ok(RawField { ty, name: name.to_string(), count, bit_width })
}

impl ParamDef {
    /// Parses a PARAMDEF XML document and computes its layout.
    pub fn parse(xml: &str) -> Result<Self, Error> {
        let xml = xml.trim_start_matches('\u{feff}');
        let doc = roxmltree::Document::parse(xml).map_err(|e| err!("xml: {e}"))?;
        let root = doc.root_element();
        let param_type =
            root.children().find(|n| n.has_tag_name("ParamType")).and_then(|n| n.text()).ok_or_else(|| err!("missing <ParamType>"))?.trim().to_string();

        let fields_node = root.children().find(|n| n.has_tag_name("Fields")).ok_or_else(|| err!("missing <Fields>"))?;

        let mut fields = Vec::new();
        let mut offset = 0usize;
        // Current bitfield storage unit: (byte offset, unit size in bytes, bits used).
        let mut unit: Option<(usize, usize, u8)> = None;

        for node in fields_node.children().filter(|n| n.has_tag_name("Field")) {
            let def = node.attribute("Def").ok_or_else(|| err!("<Field> without Def"))?;
            let raw = parse_def(def)?;

            match raw.bit_width {
                Some(width) => {
                    if raw.count != 1 {
                        return Err(err!("bitfield arrays are not supported: {def:?}"));
                    }
                    let unit_size = raw.ty.size();
                    let unit_bits = (unit_size * 8) as u8;
                    // Start a new storage unit if there is none, it is a different size,
                    // or this field does not fit in the remaining bits.
                    let needs_new = match unit {
                        Some((_, size, used)) => size != unit_size || used + width > unit_bits,
                        None => true,
                    };
                    if needs_new {
                        unit = Some((offset, unit_size, 0));
                        offset += unit_size;
                    }
                    let (unit_offset, _, used) = unit.as_mut().unwrap();
                    let limits = limits_of(&node, raw.ty);
                    fields.push(Field { name: raw.name, ty: raw.ty, offset: *unit_offset, count: 1, bits: Some((*used, width)), limits });
                    *used += width;
                }
                None => {
                    unit = None;
                    let limits = limits_of(&node, raw.ty);
                    fields.push(Field { name: raw.name, ty: raw.ty, offset, count: raw.count, bits: None, limits });
                    offset += raw.ty.size() * raw.count;
                }
            }
        }

        let mut by_name = HashMap::new();
        let mut by_lower_name = HashMap::new();
        for (i, f) in fields.iter().enumerate() {
            by_name.entry(f.name.clone()).or_insert(i);
            by_lower_name.entry(f.name.to_ascii_lowercase()).or_insert(i);
        }

        Ok(Self { param_type, fields, size: offset, by_name, by_lower_name })
    }

    /// Finds a field by its exact name, falling back to a case-insensitive match.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.by_name.get(name).or_else(|| self.by_lower_name.get(&name.to_ascii_lowercase())).map(|&i| &self.fields[i])
    }

    /// Reads `field` from a row's bytes. `row` must be at least [`ParamDef::size`] long.
    pub fn read(&self, field: &Field, row: &[u8]) -> Value {
        let at = &row[field.offset..];
        if let Some((bit, width)) = field.bits {
            let unit = read_uint(at, field.ty.size());
            let mask = if width >= 64 { u64::MAX } else { (1u64 << width) - 1 };
            return Value::Int(((unit >> bit) & mask) as i64);
        }
        match field.ty {
            FieldType::FixStr => {
                let bytes = &at[..field.count];
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                Value::Str(String::from_utf8_lossy(&bytes[..end]).into_owned())
            }
            FieldType::FixStrW => {
                let units: Vec<u16> = (0..field.count).map(|i| u16::from_le_bytes([at[i * 2], at[i * 2 + 1]])).take_while(|&u| u != 0).collect();
                Value::Str(String::from_utf16_lossy(&units))
            }
            _ if field.count != 1 => Value::Bytes(at[..field.ty.size() * field.count].to_vec()),
            FieldType::F32 => Value::Float(f32::from_le_bytes(at[..4].try_into().unwrap())),
            FieldType::U8 | FieldType::Dummy8 => Value::Int(at[0] as i64),
            FieldType::S8 => Value::Int(at[0] as i8 as i64),
            FieldType::U16 => Value::Int(u16::from_le_bytes([at[0], at[1]]) as i64),
            FieldType::S16 => Value::Int(i16::from_le_bytes([at[0], at[1]]) as i64),
            FieldType::U32 => Value::Int(u32::from_le_bytes(at[..4].try_into().unwrap()) as i64),
            FieldType::S32 => Value::Int(i32::from_le_bytes(at[..4].try_into().unwrap()) as i64),
        }
    }

    /// Writes `value` into `field` of a row's bytes, range-checking integers.
    /// Only scalar fields (numbers and bitfields) can be written.
    /// Whether `value` is within the field's limits (its paramdef's `<Minimum>`..`<Maximum>`);
    /// a field without them takes any value its type holds (`write` checks that).
    pub fn check(&self, field: &Field, value: &Value) -> Result<(), Error> {
        let Some((lo, hi)) = field.limits else { return Ok(()) };
        let v = match value {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f as f64,
            _ => return Ok(()),
        };
        if (lo..=hi).contains(&v) { Ok(()) } else { Err(err!("{v} is outside {}'s range {lo}..={hi} (its paramdef)", field.name)) }
    }

    pub fn write(&self, field: &Field, row: &mut [u8], value: &Value) -> Result<(), Error> {
        if !field.is_scalar() {
            return Err(err!("field {} ({}[{}]) is not writable", field.name, field.ty.name(), field.count));
        }
        let at = &mut row[field.offset..];

        if field.ty == FieldType::F32 {
            let v = match value {
                Value::Float(f) => *f,
                Value::Int(i) => *i as f32,
                other => return Err(err!("field {} expects a number, got {other}", field.name)),
            };
            at[..4].copy_from_slice(&v.to_le_bytes());
            return Ok(());
        }

        let v = match value {
            Value::Int(i) => *i,
            Value::Float(f) if f.fract() == 0.0 => *f as i64,
            other => return Err(err!("field {} expects an integer, got {other}", field.name)),
        };

        if let Some((bit, width)) = field.bits {
            let max = (1i64 << width) - 1;
            if !(0..=max).contains(&v) {
                return Err(err!("{} is out of range for {}-bit field {}", v, width, field.name));
            }
            let size = field.ty.size();
            let mask = ((1u64 << width) - 1) << bit;
            let unit = (read_uint(at, size) & !mask) | (((v as u64) << bit) & mask);
            write_uint(at, size, unit);
            return Ok(());
        }

        let (min, max): (i64, i64) = match field.ty {
            FieldType::U8 | FieldType::Dummy8 => (0, u8::MAX as i64),
            FieldType::S8 => (i8::MIN as i64, i8::MAX as i64),
            FieldType::U16 => (0, u16::MAX as i64),
            FieldType::S16 => (i16::MIN as i64, i16::MAX as i64),
            FieldType::U32 => (0, u32::MAX as i64),
            FieldType::S32 => (i32::MIN as i64, i32::MAX as i64),
            FieldType::F32 | FieldType::FixStr | FieldType::FixStrW => unreachable!(),
        };
        if !(min..=max).contains(&v) {
            return Err(err!("{} is out of range for {} field {} ({}..={})", v, field.ty.name(), field.name, min, max));
        }
        write_uint(at, field.ty.size(), v as u64);
        Ok(())
    }
}

fn read_uint(at: &[u8], size: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&at[..size]);
    u64::from_le_bytes(buf)
}

fn write_uint(at: &mut [u8], size: usize, v: u64) {
    at[..size].copy_from_slice(&v.to_le_bytes()[..size]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PARAMDEF XmlVersion="0">
  <ParamType>TEST_ST</ParamType>
  <Fields>
    <Field Def="s32 id = -1" />
    <Field Def="f32 weight" />
    <Field Def="u8 a:1 = 1" />
    <Field Def="u8 b:3" />
    <Field Def="u8 c:5" />
    <Field Def="s16 icon" />
    <Field Def="dummy8 pad[3]" />
    <Field Def="fixstrW label[4]" />
  </Fields>
</PARAMDEF>"#;

    #[test]
    fn layout() {
        let def = ParamDef::parse(XML).unwrap();
        assert_eq!(def.param_type, "TEST_ST");
        let off = |n: &str| def.field(n).unwrap().offset;
        assert_eq!(off("id"), 0);
        assert_eq!(off("weight"), 4);
        // a and b share byte 8; c doesn't fit (1+3+5 > 8) so it starts byte 9
        assert_eq!((off("a"), def.field("a").unwrap().bits), (8, Some((0, 1))));
        assert_eq!((off("b"), def.field("b").unwrap().bits), (8, Some((1, 3))));
        assert_eq!((off("c"), def.field("c").unwrap().bits), (9, Some((0, 5))));
        assert_eq!(off("icon"), 10);
        assert_eq!(off("pad"), 12);
        assert_eq!(off("label"), 15);
        assert_eq!(def.size, 23);
        assert!(def.field("WEIGHT").is_some(), "case-insensitive lookup");
    }

    #[test]
    fn read_write_roundtrip() {
        let def = ParamDef::parse(XML).unwrap();
        let mut row = vec![0u8; def.size];
        let f = |n: &str| def.field(n).unwrap().clone();

        def.write(&f("id"), &mut row, &Value::Int(-5)).unwrap();
        def.write(&f("weight"), &mut row, &Value::Float(1.5)).unwrap();
        def.write(&f("b"), &mut row, &Value::Int(5)).unwrap();
        def.write(&f("a"), &mut row, &Value::Int(1)).unwrap();
        assert_eq!(def.read(&f("id"), &row), Value::Int(-5));
        assert_eq!(def.read(&f("weight"), &row), Value::Float(1.5));
        assert_eq!(def.read(&f("b"), &row), Value::Int(5));
        assert_eq!(def.read(&f("a"), &row), Value::Int(1));
        assert_eq!(row[8], 0b1011, "bits packed LSB-first");

        assert!(def.write(&f("b"), &mut row, &Value::Int(8)).is_err(), "3-bit overflow");
        assert!(def.write(&f("icon"), &mut row, &Value::Int(40000)).is_err(), "s16 overflow");
        assert!(def.write(&f("label"), &mut row, &Value::Int(1)).is_err(), "strings read-only");
    }

    #[test]
    fn limits_from_the_paramdef() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<PARAMDEF XmlVersion="3">
  <ParamType>LIMITS_TEST_ST</ParamType>
  <DataVersion>1</DataVersion>
  <BigEndian>False</BigEndian>
  <Unicode>True</Unicode>
  <FormatVersion>203</FormatVersion>
  <Fields>
    <Field Def="u8 kind"><Maximum>3</Maximum></Field>
    <Field Def="s32 lot"><Minimum>-1</Minimum><Maximum>999999999</Maximum></Field>
    <Field Def="u32 flag"><Maximum>-294967297</Maximum></Field>
    <Field Def="s32 grayoutFlag"><DisplayFormat>%u</DisplayFormat><Maximum>-294967297</Maximum></Field>
    <Field Def="f32 scale" />
  </Fields>
</PARAMDEF>"#;
        let def = ParamDef::parse(xml).unwrap();
        let f = |n: &str| def.field(n).unwrap().clone();
        assert_eq!(f("kind").limits, Some((0.0, 3.0)));
        assert!(def.check(&f("kind"), &Value::Int(3)).is_ok());
        assert!(def.check(&f("kind"), &Value::Int(4)).is_err());
        assert!(def.check(&f("lot"), &Value::Int(-1)).is_ok() && def.check(&f("lot"), &Value::Int(-2)).is_err());
        // A wrapped limit (a negative maximum on a field shown unsigned) is left out.
        assert_eq!(f("flag").limits, None);
        assert!(def.check(&f("flag"), &Value::Int(4_000_000_000)).is_ok());
        // ActionButtonParam's s32 flags shown as %u: a flag (Cemetery of Ash's 14000950) and the
        // game's own -1 both pass.
        assert_eq!(f("grayoutFlag").limits, None);
        assert!(def.check(&f("grayoutFlag"), &Value::Int(14000950)).is_ok() && def.check(&f("grayoutFlag"), &Value::Int(-1)).is_ok());
        assert_eq!(f("scale").limits, None);
    }
}
