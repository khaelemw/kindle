//! Dark Souls III's param tables for Kindle's registry (kindle-core's params.rs, re-exported here):
//! the paramdefs embedded in the DLL (build.rs) and the regulation's loaded tables.

pub use kindle_core::params::*;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/paramdefs.rs"));
}

/// Dark Souls III's paramdefs (file name, XML), for the bindings.
pub use embedded::PARAMDEFS;

/// The regulation's tables now (None until it's loaded), for the bindings.
pub fn caps() -> Option<Vec<ParamCap>> {
    let caps = crate::ds3::param_caps()?;
    Some(caps.into_iter().map(|c| ParamCap { name: c.name, table: c.table, table_size: c.table_size }).collect())
}
