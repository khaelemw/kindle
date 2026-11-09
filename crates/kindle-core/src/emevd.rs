//! Event script (EMEVD) commands' arguments, as the games' event files lay them out: each game's
//! crate runs a mod's command through its own dispatcher with arguments packed here.

/// Packs arguments like the event files do: each value aligned to its own size. `format`
/// has one letter per value: b/B i8/u8, h/H i16/u16, i/I i32/u32, f f32.
pub fn pack(format: &str, values: &[f64]) -> Result<Vec<u8>, String> {
    if format.chars().count() != values.len() {
        return Err(format!("the format \"{format}\" has {} letters but {} values were given", format.chars().count(), values.len()));
    }
    let mut out = Vec::new();
    for (c, &v) in format.chars().zip(values) {
        let size = match c {
            'b' | 'B' => 1,
            'h' | 'H' => 2,
            'i' | 'I' | 'f' => 4,
            other => return Err(format!("unknown format letter '{other}' (use b B h H i I f)")),
        };
        while out.len() % size != 0 {
            out.push(0);
        }
        match c {
            'b' => out.push(v as i8 as u8),
            'B' => out.push(v as u8),
            'h' => out.extend((v as i16).to_le_bytes()),
            'H' => out.extend((v as u16).to_le_bytes()),
            'i' => out.extend((v as i32).to_le_bytes()),
            'I' => out.extend((v as u32).to_le_bytes()),
            _ => out.extend((v as f32).to_le_bytes()),
        }
    }
    while out.len() % 4 != 0 {
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::pack;

    #[test]
    fn args_are_aligned_like_event_files() {
        // DisplayGenericDialog-style: i32, u16, u16, i32, f32
        let a = pack("iHHif", &[10010170.0, 1.0, 2.0, 0.0, 3.0]).unwrap();
        assert_eq!(a.len(), 16);
        assert_eq!(&a[4..6], &1u16.to_le_bytes());
        // a byte then an int: the int starts at 4
        let b = pack("Bi", &[1.0, 7.0]).unwrap();
        assert_eq!(b, vec![1, 0, 0, 0, 7, 0, 0, 0]);
        assert!(pack("i", &[]).is_err());
        assert!(pack("x", &[1.0]).is_err());
    }
}
