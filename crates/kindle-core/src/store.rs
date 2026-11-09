//! Persistent storage for mods (`kindle.store`).
//!
//! Each mod gets two stores, kept as JSON under `<kindle>/data/<mod id>/`: one per save
//! slot (`slot<N>.json`, for the character's progress) and one for the mod itself
//! (`global.json`, for settings). Mods never touch files: they get and set values, and
//! Kindle writes the files (at most once a second, and when the mod unloads or the
//! character leaves the world). A slot file remembers whose it was; if another character
//! now has that slot, the store starts empty and the old file is kept as a backup.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mlua::{Lua, Value as LuaValue};
use serde_json::{Map, Value};

static ROOT: OnceLock<PathBuf> = OnceLock::new();
/// Largest value one `set` may store (serialized), and the deepest nesting.
const MAX_VALUE_BYTES: usize = 1 << 20;
const MAX_DEPTH: usize = 32;

/// Sets where stores live (`<kindle>/data`), once at startup.
pub fn set_root(kindle_dir: &Path) {
    let _ = ROOT.set(kindle_dir.join("data"));
}

pub struct Store {
    path: PathBuf,
    /// For slot stores: the character the data belongs to.
    character: Option<String>,
    values: Map<String, Value>,
    dirty: bool,
}

impl Store {
    fn dir(mod_id: &str) -> Result<PathBuf, String> {
        Ok(ROOT.get().ok_or("storage isn't set up yet")?.join(mod_id))
    }

    fn read(path: &Path) -> Option<Map<String, Value>> {
        let text = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(o)) => Some(o),
            _ => {
                crate::warn!("store: {} is unreadable; starting empty (the file is kept as .bad)", path.display());
                let _ = std::fs::rename(path, path.with_extension("json.bad"));
                None
            }
        }
    }

    /// The mod's global store.
    pub fn global(mod_id: &str) -> Result<Store, String> {
        Self::named(mod_id, "global")
    }

    /// A store of the mod's own in `<name>.json` (global.json, settings.json).
    pub fn named(mod_id: &str, name: &str) -> Result<Store, String> {
        let path = Self::dir(mod_id)?.join(format!("{name}.json"));
        let values = Self::read(&path).and_then(|mut o| match o.remove("values") {
            Some(Value::Object(v)) => Some(v),
            _ => None,
        });
        Ok(Store { path, character: None, values: values.unwrap_or_default(), dirty: false })
    }

    /// The store for the character in `slot`.
    pub fn character(mod_id: &str, slot: i32, character: &str) -> Result<Store, String> {
        let path = Self::dir(mod_id)?.join(format!("slot{slot}.json"));
        let mut values = Map::new();
        if let Some(mut o) = Self::read(&path) {
            let owner = o.get("character").and_then(Value::as_str).unwrap_or("").to_string();
            if owner == character {
                if let Some(Value::Object(v)) = o.remove("values") {
                    values = v;
                }
            } else {
                let backup = path.with_extension(format!("{}.json", sanitize(&owner)));
                crate::info!("store: slot {slot} now holds {character:?}; {owner:?}'s data kept in {}", backup.display());
                let _ = std::fs::rename(&path, backup);
            }
        }
        Ok(Store { path, character: Some(character.to_string()), values, dirty: false })
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn keys(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }

    pub fn set(&mut self, key: &str, value: Option<Value>) {
        match value {
            Some(v) => {
                self.values.insert(key.to_string(), v);
            }
            None => {
                self.values.remove(key);
            }
        }
        self.dirty = true;
    }

    /// Writes the file if anything changed (through a temporary file, so a crash mid-write
    /// never leaves a half-written store).
    pub fn flush(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        self.dirty = false;
        let mut doc = Map::new();
        if let Some(c) = &self.character {
            doc.insert("character".into(), Value::String(c.clone()));
        }
        doc.insert("values".into(), Value::Object(self.values.clone()));
        let text = serde_json::to_string_pretty(&Value::Object(doc)).map_err(|e| e.to_string())?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            crate::warn!("store: {e}");
        }
    }
}

/// A name usable in a file name.
fn sanitize(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if out.is_empty() { "unnamed".into() } else { out }
}

/// A Lua value as JSON: nil (None), booleans, numbers, strings, and tables that are lists or
/// have string keys.
pub fn to_json(v: &LuaValue) -> Result<Option<Value>, String> {
    let json = convert(v, 0)?;
    if let Some(j) = &json {
        let size = serde_json::to_string(j).map(|s| s.len()).unwrap_or(0);
        if size > MAX_VALUE_BYTES {
            return Err(format!("value is too large to store ({size} bytes; at most {MAX_VALUE_BYTES})"));
        }
    }
    Ok(json)
}

fn convert(v: &LuaValue, depth: usize) -> Result<Option<Value>, String> {
    if depth > MAX_DEPTH {
        return Err(format!("tables nest too deep to store (at most {MAX_DEPTH} levels, or a table contains itself)"));
    }
    Ok(Some(match v {
        LuaValue::Nil => return Ok(None),
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::from(*i),
        LuaValue::Number(n) => serde_json::Number::from_f64(*n).map(Value::Number).ok_or("NaN and infinity can't be stored")?,
        LuaValue::String(s) => Value::String(s.to_string_lossy()),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            let mut count = 0usize;
            let mut all_strings = true;
            for pair in t.pairs::<LuaValue, LuaValue>() {
                let (k, _) = pair.map_err(|e| e.to_string())?;
                count += 1;
                all_strings &= matches!(k, LuaValue::String(_));
            }
            if count > 0 && count == len {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let item: LuaValue = t.raw_get(i).map_err(|e| e.to_string())?;
                    arr.push(convert(&item, depth + 1)?.unwrap_or(Value::Null));
                }
                Value::Array(arr)
            } else if all_strings {
                let mut obj = Map::new();
                for pair in t.pairs::<String, LuaValue>() {
                    let (k, item) = pair.map_err(|e| e.to_string())?;
                    if let Some(j) = convert(&item, depth + 1)? {
                        obj.insert(k, j);
                    }
                }
                Value::Object(obj)
            } else {
                return Err("tables to store must be lists or have string keys".into());
            }
        }
        other => return Err(format!("a {} can't be stored (only nil, booleans, numbers, strings and tables)", other.type_name())),
    }))
}

/// JSON back to a Lua value.
pub fn to_lua(lua: &Lua, v: &Value) -> mlua::Result<LuaValue> {
    Ok(match v {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => LuaValue::Integer(i),
            None => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => LuaValue::String(lua.create_string(s)?),
        Value::Array(a) => {
            let t = lua.create_table_with_capacity(a.len(), 0)?;
            for item in a {
                t.raw_push(to_lua(lua, item)?)?;
            }
            LuaValue::Table(t)
        }
        Value::Object(o) => {
            let t = lua.create_table_with_capacity(0, o.len())?;
            for (k, item) in o {
                t.raw_set(k.as_str(), to_lua(lua, item)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kindle-store-test-{}", std::process::id()));
        let _ = ROOT.set(dir.join("data"));
        ROOT.get().unwrap().clone()
    }

    #[test]
    fn values_round_trip_through_json() {
        let lua = Lua::new();
        let v: LuaValue =
            lua.load(r#"return { name = "fate", level = 3, ratio = 0.5, on = true, list = {1, 2, "x"}, nested = { a = { b = {} } } }"#).eval().unwrap();
        let json = to_json(&v).unwrap().unwrap();
        let back = to_lua(&lua, &json).unwrap();
        let t = back.as_table().unwrap();
        assert_eq!(t.get::<String>("name").unwrap(), "fate");
        assert_eq!(t.get::<i64>("level").unwrap(), 3);
        assert_eq!(t.get::<f64>("ratio").unwrap(), 0.5);
        assert_eq!(t.get::<mlua::Table>("list").unwrap().raw_len(), 3);
        // mixed keys and functions are refused
        let bad: LuaValue = lua.load("return { 1, x = 2 }").eval().unwrap();
        assert!(to_json(&bad).is_err());
        let f: LuaValue = lua.load("return print").eval().unwrap();
        assert!(to_json(&f).is_err());
        let cyclic: LuaValue = lua.load("local t = {} t.self = t return t").eval().unwrap();
        assert!(to_json(&cyclic).is_err());
    }

    #[test]
    fn slot_store_persists_and_detects_a_new_character() {
        let root = temp_root();
        let id = "store-test";
        {
            let mut s = Store::character(id, 2, "Ashen One").unwrap();
            s.set("fate", Some(Value::from("roll")));
        } // dropped: flushed
        let s = Store::character(id, 2, "Ashen One").unwrap();
        assert_eq!(s.get("fate"), Some(&Value::from("roll")));
        drop(s);
        let other = Store::character(id, 2, "Someone Else").unwrap();
        assert!(other.get("fate").is_none());
        assert!(root.join(id).join("slot2.Ashen_One.json").exists());
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }
}
