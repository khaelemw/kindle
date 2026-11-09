//! By-name access to the game's param tables.
//!
//! The registry pairs every loaded table (the game's bindings name them: `param_caps`) with the
//! paramdef the game's DLL embeds for its type (`paramdefs`), and validates the computed row size
//! against the real row stride in memory. A table that fails validation is read-only, so a wrong
//! layout can never corrupt memory.
//!
//! A table is the param file as the game loaded it, the same in Dark Souls III and Elden Ring: a
//! 0x40-byte header (the row count at +0x0A, the type name's offset at +0x10), then an index of
//! {u64 id, u64 data offset, u64 name offset} per row, then the rows' data and names.

use std::collections::{BTreeSet, HashMap};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use kindle_paramdef::{Field, FieldType, ParamDef, Value};

use crate::memory;

/// One of the game's loaded tables, as its bindings give it.
pub struct ParamCap {
    /// Camel-case table name as the game knows it, e.g. `EquipParamGoods`.
    pub name: String,
    /// The table in the game's memory.
    pub table: usize,
    pub table_size: usize,
}

/// A game's way of adding rows to a table (the bindings' `add_param_rows`): the mod adding them,
/// the table, then (id, copy_from) for each row.
pub type AddRows = fn(&str, &str, &[(u64, u64)]) -> Result<(), String>;

/// A param table's type name (e.g. "EQUIP_PARAM_GOODS_ST"): a C string at the offset the
/// header holds at +0x10.
pub fn table_type(table: usize) -> Option<String> {
    let off = memory::read_u64(table + 0x10)? as usize;
    let bytes = memory::read_bytes(table + off, 64)?;
    let end = bytes.iter().position(|&b| b == 0)?;
    String::from_utf8(bytes[..end].to_vec()).ok()
}

/// A param table's rows in table order: (row id, data offset from the table). The header
/// holds the count at +0x0A; the index at +0x40 has {u64 id, u64 offset, u64 name offset}.
pub fn table_rows(table: usize) -> Option<Vec<(u64, usize)>> {
    let count = memory::read::<u16>(table + 0x0A)? as usize;
    let index = memory::read_bytes(table + 0x40, count * 0x18)?;
    Some(
        index
            .as_chunks::<0x18>()
            .0
            .iter()
            .map(|e| (u64::from_le_bytes(e[0..8].try_into().unwrap()), u64::from_le_bytes(e[8..16].try_into().unwrap()) as usize))
            .collect(),
    )
}

pub struct Table {
    /// Camel-case table name as the game knows it, e.g. `EquipParamGoods`.
    pub name: String,
    pub def: Arc<ParamDef>,
    /// The table in the game's memory.
    base: NonNull<u8>,
    /// row id → byte offset from the table base.
    rows: HashMap<u64, usize>,
    /// Row ids in table order.
    pub row_ids: Vec<u64>,
    /// False if the layout check failed; such tables are never read or written.
    pub valid: bool,
    pub note: Option<String>,
}

impl Table {
    fn row_ptr(&self, id: u64) -> Option<*mut u8> {
        let off = *self.rows.get(&id)?;
        Some(unsafe { self.base.as_ptr().add(off) })
    }

    /// The row's bytes. They are the game's param memory, not part of `self`.
    #[allow(clippy::mut_from_ref)]
    fn row_bytes(&self, id: u64) -> Result<&mut [u8], String> {
        if !self.valid {
            return Err(format!("{} is unavailable: {}", self.name, self.note.as_deref().unwrap_or("layout check failed")));
        }
        let p = self.row_ptr(id).ok_or_else(|| format!("{} has no row {id}", self.name))?;
        Ok(unsafe { std::slice::from_raw_parts_mut(p, self.def.size) })
    }

    /// A copy of a row's bytes.
    pub fn row_data(&self, id: u64) -> Result<Vec<u8>, String> {
        Ok(self.row_bytes(id)?.to_vec())
    }

    /// Writes `bytes` (a row of this table) over row `id`, keeping its own values of the
    /// fields in `keep`.
    pub fn write_row(&self, id: u64, bytes: &[u8], keep: &[&str]) -> Result<(), String> {
        let kept: Vec<(&Field, Value)> = keep.iter().map(|&n| Ok((self.field(n)?, self.read(id, n)?))).collect::<Result<_, String>>()?;
        let row = self.row_bytes(id)?;
        if bytes.len() != row.len() {
            return Err(format!("{}: a row of {} bytes, not {}", self.name, bytes.len(), row.len()));
        }
        row.copy_from_slice(bytes);
        for (f, v) in kept {
            self.def.write(f, row, &v).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn has_row(&self, id: u64) -> bool {
        self.rows.contains_key(&id)
    }

    pub fn field(&self, name: &str) -> Result<&Field, String> {
        self.def.field(name).ok_or_else(|| format!("{} has no field {name:?} (see params.fields({:?}))", self.name, self.name))
    }

    pub fn read(&self, id: u64, field: &str) -> Result<Value, String> {
        let f = self.field(field)?;
        let row = self.row_bytes(id)?;
        Ok(self.def.read(f, row))
    }

    /// A mod's write: as `write`, within the field's limits from its paramdef (a value out of them,
    /// an index the game never expected, could make the game read or write outside its tables).
    /// The value the field already has is always allowed (the game's own data has a few outside),
    /// as is the id of a row a mod added, in a 32-bit field: new rows go above the game's, which
    /// can be above the limit of the fields that name them (Elden Ring's weapons end at 99999999,
    /// the most its originEquipWep fields hold by their paramdef).
    pub fn write_checked(&self, id: u64, field: &str, value: &Value) -> Result<Value, String> {
        let f = self.field(field)?;
        if self.read(id, field)? != *value && !names_added_row(f, value) {
            self.def.check(f, value).map_err(|e| format!("{}: {e}", self.name))?;
        }
        self.write(id, field, value)
    }

    /// Writes a field and returns the value it had before.
    pub fn write(&self, id: u64, field: &str, value: &Value) -> Result<Value, String> {
        let f = self.field(field)?;
        let row = self.row_bytes(id)?;
        let old = self.def.read(f, row);
        self.def.write(f, row, value).map_err(|e| e.to_string())?;
        Ok(old)
    }
}

pub struct Registry {
    tables: Vec<Table>,
    by_lower_name: HashMap<String, usize>,
}

// The registry holds raw pointers into game memory. It is only ever used from the game's
// task thread (all Lua runs there); the Mutex below exists to satisfy Rust, not for
// cross-thread sharing of the pointers.
unsafe impl Send for Registry {}
unsafe impl Sync for Registry {}

static REGISTRY: Mutex<Option<Arc<Registry>>> = Mutex::new(None);

/// The ids of the rows mods added this session, in any table (they stay as long as the game).
static ADDED: Mutex<BTreeSet<u64>> = Mutex::new(BTreeSet::new());

/// Rows a mod added: fields may name them whatever their limits (`write_checked`).
pub fn note_added(ids: impl IntoIterator<Item = u64>) {
    ADDED.lock().unwrap_or_else(|e| e.into_inner()).extend(ids);
}

/// Whether `value`, written to `field`, is the id of a row a mod added: a whole 32-bit field (a
/// row's id is one), not a bitfield or a narrower one.
fn names_added_row(field: &Field, value: &Value) -> bool {
    matches!(field.ty, FieldType::S32 | FieldType::U32)
        && field.bits.is_none()
        && matches!(value, Value::Int(v) if u64::try_from(*v).is_ok_and(|v| ADDED.lock().unwrap_or_else(|e| e.into_inner()).contains(&v)))
}

fn load_defs(paramdefs: &[(&str, &str)]) -> HashMap<String, Arc<ParamDef>> {
    let mut defs = HashMap::new();
    for (file, xml) in paramdefs {
        match ParamDef::parse(xml) {
            Ok(def) => {
                defs.insert(def.param_type.clone(), Arc::new(def));
            }
            Err(e) => crate::error!("paramdef {file}: {e}"),
        }
    }
    defs
}

impl Registry {
    fn build() -> Result<Self, String> {
        let bindings = crate::bindings::get().ok_or("no game's bindings are set")?;
        let caps = (bindings.param_caps)().filter(|c| !c.is_empty()).ok_or("params are not loaded yet")?;
        let defs = load_defs(bindings.paramdefs);
        let mut tables = Vec::new();
        let mut unknown = Vec::new();

        for cap in caps {
            let name = cap.name;
            let Some(def_type) = table_type(cap.table) else {
                unknown.push(format!("{name} (unreadable type)"));
                continue;
            };
            let Some(def) = defs.get(&def_type).cloned() else {
                unknown.push(format!("{name} ({def_type})"));
                continue;
            };

            let infos = table_rows(cap.table).unwrap_or_default();
            let mut rows = HashMap::with_capacity(infos.len());
            let mut row_ids = Vec::with_capacity(infos.len());
            for &(id, offset) in &infos {
                rows.entry(id).or_insert(offset);
                row_ids.push(id);
            }

            // Validate: consecutive row offsets must be at least one row apart, and every
            // row must lie inside the table's allocation.
            let table_size = cap.table_size;
            let mut offsets: Vec<usize> = infos.iter().map(|&(_, o)| o).collect();
            offsets.sort_unstable();
            offsets.dedup();
            let mut note = None;
            if let Some(w) = offsets.windows(2).find(|w| w[1] - w[0] < def.size) {
                note = Some(format!("row stride {} is smaller than the paramdef row size {}", w[1] - w[0], def.size));
            } else if let Some(&last) = offsets.last()
                && last + def.size > table_size
            {
                note = Some(format!("last row ends at {} but the table is only {} bytes", last + def.size, table_size));
            }
            if let Some(n) = &note {
                crate::warn!("param table {name}: {n}; it will be unavailable to scripts");
            }

            tables.push(Table {
                name,
                def,
                base: NonNull::new(cap.table as *mut u8).ok_or("a param table is null")?,
                rows,
                row_ids,
                valid: note.is_none(),
                note,
            });
        }

        if !unknown.is_empty() {
            crate::debug!("param tables without a paramdef: {}", unknown.join(", "));
        }

        let by_lower_name = tables.iter().enumerate().map(|(i, t)| (t.name.to_ascii_lowercase(), i)).collect();
        let valid = tables.iter().filter(|t| t.valid).count();
        crate::info!("params: {} tables mapped ({valid} validated)", tables.len());
        Ok(Self { tables, by_lower_name })
    }

    pub fn table(&self, name: &str) -> Result<&Table, String> {
        self.by_lower_name
            .get(&name.to_ascii_lowercase())
            .map(|&i| &self.tables[i])
            .ok_or_else(|| format!("no param table named {name:?} (see params.tables())"))
    }

    pub fn tables(&self) -> impl Iterator<Item = &Table> {
        self.tables.iter()
    }
}

/// Drops the registry so the next use rebuilds it (tables were replaced).
pub fn invalidate() {
    *REGISTRY.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The param registry, built on first use once the game has loaded its params.
pub fn registry() -> Result<Arc<Registry>, String> {
    let mut slot = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(r) = slot.as_ref() {
        return Ok(r.clone());
    }
    let r = Arc::new(Registry::build()?);
    *slot = Some(r.clone());
    Ok(r)
}

/// A mod has unloaded: puts back each param field it wrote (its journal: each field's value before
/// the mod's first write).
pub fn revert_journal(id: &str, journal: &mut HashMap<(String, u64, String), Value>) {
    if journal.is_empty() {
        return;
    }
    let Ok(reg) = registry() else { return };
    let mut reverted = 0;
    for ((table, row, field), original) in journal.drain() {
        if let Ok(t) = reg.table(&table)
            && t.write(row, &field, &original).is_ok()
        {
            reverted += 1;
        }
    }
    crate::log::write(crate::log::Level::Info, id, &format!("reverted {reverted} param edit(s)"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field naming a weapon, with Elden Ring's limits for it, and a narrow one.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<PARAMDEF XmlVersion="3">
  <ParamType>WRITE_TEST_ST</ParamType>
  <DataVersion>1</DataVersion>
  <BigEndian>False</BigEndian>
  <Unicode>True</Unicode>
  <FormatVersion>203</FormatVersion>
  <Fields>
    <Field Def="s32 originEquipWep"><Minimum>-1</Minimum><Maximum>99999999</Maximum></Field>
    <Field Def="u8 kind"><Maximum>3</Maximum></Field>
    <Field Def="dummy8 pad[3]" />
  </Fields>
</PARAMDEF>"#;

    #[test]
    fn a_field_may_name_a_row_a_mod_added_above_its_limits() {
        let def = Arc::new(ParamDef::parse(XML).unwrap());
        let mut data = vec![0u8; 2 * def.size];
        let table = Table {
            name: "WriteTest".into(),
            base: NonNull::new(data.as_mut_ptr()).unwrap(),
            rows: HashMap::from([(1000000, 0), (100000000, def.size)]),
            row_ids: vec![1000000, 100000000],
            def,
            valid: true,
            note: None,
        };
        let e = table.write_checked(1000000, "originEquipWep", &Value::Int(100000000)).unwrap_err();
        assert!(e.contains("outside originEquipWep's range"), "{e}");

        note_added([100000000, 200]);
        table.write_checked(100000000, "originEquipWep", &Value::Int(100000000)).unwrap();
        assert_eq!(table.read(100000000, "originEquipWep").unwrap(), Value::Int(100000000));
        // Only an added row's id, and only in a whole 32-bit field.
        assert!(table.write_checked(100000000, "originEquipWep", &Value::Int(100010000)).is_err());
        assert!(table.write_checked(100000000, "kind", &Value::Int(200)).is_err());
        table.write_checked(100000000, "kind", &Value::Int(3)).unwrap();
    }
}
