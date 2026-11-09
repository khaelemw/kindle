//! JSON for mods (kindle.json): Lua values to JSON text and back, through serde_json.
//!
//! A table whose keys are 1..n with no others is a JSON array; any other table is an object
//! (number keys become text). An empty table is `[]`. JSON's null reads as nil.

use mlua::{Lua, Table, Value};
use serde_json::{Map, Number, Value as Json};

/// How deep tables may nest (a table inside itself would never end).
const MAX_DEPTH: usize = 100;

fn to_json(v: &Value, depth: usize) -> Result<Json, String> {
    if depth > MAX_DEPTH {
        return Err(format!("nested more than {MAX_DEPTH} deep (a table inside itself?)"));
    }
    Ok(match v {
        Value::Nil => Json::Null,
        Value::Boolean(b) => Json::Bool(*b),
        Value::Integer(i) => Json::Number((*i).into()),
        Value::Number(f) => Json::Number(Number::from_f64(*f).ok_or("a number that isn't finite (inf or nan)")?),
        Value::String(s) => Json::String(s.to_str().map_err(|_| "text that isn't UTF-8")?.to_string()),
        Value::Table(t) => table_to_json(t, depth)?,
        other => return Err(format!("a {} can't be JSON", other.type_name())),
    })
}

fn table_to_json(t: &Table, depth: usize) -> Result<Json, String> {
    let n = t.raw_len();
    let mut count = 0;
    for pair in t.clone().pairs::<Value, Value>() {
        pair.map_err(|e| e.to_string())?;
        count += 1;
    }
    if count == n {
        let mut list = Vec::with_capacity(n);
        for i in 1..=n {
            list.push(to_json(&t.raw_get::<Value>(i).map_err(|e| e.to_string())?, depth + 1)?);
        }
        return Ok(Json::Array(list));
    }
    let mut map = Map::new();
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        let key = match &k {
            Value::String(s) => s.to_str().map_err(|_| "a key that isn't UTF-8")?.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Number(f) => f.to_string(),
            Value::Boolean(b) => b.to_string(),
            other => return Err(format!("a {} key can't be JSON", other.type_name())),
        };
        map.insert(key, to_json(&v, depth + 1)?);
    }
    Ok(Json::Object(map))
}

/// A Lua value as JSON text (`pretty`: indented).
pub fn encode(v: &Value, pretty: bool) -> Result<String, String> {
    let json = to_json(v, 0)?;
    if pretty { serde_json::to_string_pretty(&json) } else { serde_json::to_string(&json) }.map_err(|e| e.to_string())
}

fn from_json(lua: &Lua, j: Json) -> mlua::Result<Value> {
    Ok(match j {
        Json::Null => Value::Nil,
        Json::Bool(b) => Value::Boolean(b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => Value::String(lua.create_string(&s)?),
        Json::Array(list) => {
            let t = lua.create_table_with_capacity(list.len(), 0)?;
            for (i, v) in list.into_iter().enumerate() {
                t.raw_set(i + 1, from_json(lua, v)?)?;
            }
            Value::Table(t)
        }
        Json::Object(map) => {
            let t = lua.create_table_with_capacity(0, map.len())?;
            for (k, v) in map {
                t.raw_set(k, from_json(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

/// JSON text as a Lua value.
pub fn decode(lua: &Lua, text: &str) -> mlua::Result<Value> {
    let json: Json = serde_json::from_str(text).map_err(|e| mlua::Error::runtime(format!("kindle.json.decode: {e}")))?;
    from_json(lua, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let lua = Lua::new();
        let v: Value = lua.load(r#"return { name = "Ember", count = 3, ratio = 0.5, list = { 1, 2, "three" }, empty = {}, on = true }"#).eval().unwrap();
        let text = encode(&v, false).unwrap();
        let back = decode(&lua, &text).unwrap();
        let Value::Table(t) = back else { panic!("not a table") };
        assert_eq!(t.get::<String>("name").unwrap(), "Ember");
        assert_eq!(t.get::<i64>("count").unwrap(), 3);
        assert_eq!(t.get::<f64>("ratio").unwrap(), 0.5);
        assert_eq!(t.get::<Table>("list").unwrap().get::<String>(3).unwrap(), "three");
        assert!(text.contains("\"empty\":[]"));
        assert!(t.get::<bool>("on").unwrap());
        let cyclic: Value = lua.load("local t = {}; t.me = t; return t").eval().unwrap();
        assert!(encode(&cyclic, false).unwrap_err().contains("deep"));
        assert!(decode(&lua, "{oops").is_err());
        let sparse: Value = lua.load("return { [1] = 'a', [3] = 'c' }").eval().unwrap();
        assert!(encode(&sparse, false).unwrap().starts_with('{'));
    }
}
