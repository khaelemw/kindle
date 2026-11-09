//! Natives for kindle.events: the game's event commands (changes to its event files are kindle-core's:
//! natives/event_files.rs).

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "events_run",
        lua.create_function(|lua, (bank, index, format, values, reply): (i32, i32, String, Vec<f64>, Option<Function>)| {
            let args = crate::emevd::pack(&format, &values).map_err(rt_err)?;
            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let owner = ctx(lua)?.id.clone();
            crate::emevd::run(crate::emevd::Command { id, owner, bank, index, args }).map_err(rt_err)?;
            if let Some(f) = reply {
                ctx(lua)?.handlers.replies.push((id, f));
            }
            Ok(id)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::events::all;

    #[test]
    fn stubs_list_every_event() {
        crate::bindings::set(&crate::api::BINDINGS);
        let stubs = include_str!("../../../../dist/stubs/kindle.lua");
        let alias = stubs.lines().find_map(|l| l.strip_prefix("---@alias kindle.event ")).expect("dist/stubs/kindle.lua declares the kindle.event alias");
        let in_stubs: BTreeSet<&str> = alias.split('|').map(|e| e.trim().trim_matches('"')).collect();
        let in_runtime: BTreeSet<&str> = all().collect();
        assert_eq!(in_stubs, in_runtime, "the kindle.event alias in dist/stubs/kindle.lua must list exactly the runtime's events");
    }

    #[test]
    fn docs_describe_every_event() {
        crate::bindings::set(&crate::api::BINDINGS);
        let docs = include_str!("../../../../docs/events.md");
        let missing: Vec<&str> = all().filter(|e| !docs.contains(&format!("`{e}`"))).collect();
        assert!(missing.is_empty(), "docs/events.md doesn't describe: {missing:?}");
    }

    #[test]
    fn names_are_unique() {
        crate::bindings::set(&crate::api::BINDINGS);
        let names: Vec<&str> = all().collect();
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len());
    }
}
