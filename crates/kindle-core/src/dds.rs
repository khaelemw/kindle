//! DDS textures (the game's own pictures: item icons, menu atlases) as RGBA, the first mip only.
//!
//! The header: `DDS `, then a 124-byte header (height +12, width +16, the pixel format's flags
//! +80 and four-character code +84); a `DX10` code adds 20 bytes whose first u32 is the DXGI
//! format. The game's menu textures are BC7 (DXGI 98); BC1-BC3 (DXT1/DXT5) and plain 8-bit
//! RGBA/BGRA are read too.

/// What a texture's blocks hold.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Format {
    Bc1,
    Bc2,
    Bc3,
    Bc7,
    Rgba,
    Bgra,
}

impl Format {
    fn block_bytes(self) -> usize {
        match self {
            Format::Bc1 => 8,
            Format::Bc2 | Format::Bc3 | Format::Bc7 => 16,
            Format::Rgba | Format::Bgra => 0,
        }
    }
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A DDS file's first picture as (width, height, RGBA).
pub fn decode(dds: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if dds.get(..4) != Some(b"DDS ") {
        return Err("not a DDS texture".into());
    }
    let height = u32_at(dds, 12).ok_or("a DDS header cut short")?;
    let width = u32_at(dds, 16).ok_or("a DDS header cut short")?;
    let flags = u32_at(dds, 80).ok_or("a DDS header cut short")?;
    let code = dds.get(84..88).ok_or("a DDS header cut short")?;
    let (format, data_at) = if code == b"DX10" {
        let dxgi = u32_at(dds, 128).ok_or("a DDS header cut short")?;
        let f = match dxgi {
            71 | 72 => Format::Bc1,
            74 | 75 => Format::Bc2,
            77 | 78 => Format::Bc3,
            98 | 99 => Format::Bc7,
            28 | 29 => Format::Rgba,
            87 | 91 => Format::Bgra,
            other => return Err(format!("a DDS texture of DXGI format {other}, which Kindle doesn't read")),
        };
        (f, 148)
    } else if flags & 0x4 != 0 {
        let f = match code {
            b"DXT1" => Format::Bc1,
            b"DXT2" | b"DXT3" => Format::Bc2,
            b"DXT4" | b"DXT5" => Format::Bc3,
            other => return Err(format!("a DDS texture of format {:?}, which Kindle doesn't read", String::from_utf8_lossy(other))),
        };
        (f, 128)
    } else {
        // Uncompressed: 32 bits a pixel, red first unless the red mask is the third byte.
        let red_mask = u32_at(dds, 92).ok_or("a DDS header cut short")?;
        (if red_mask == 0x00FF_0000 { Format::Bgra } else { Format::Rgba }, 128)
    };
    if width == 0 || height == 0 || width > crate::hud::MAX_IMAGE_SIDE || height > crate::hud::MAX_IMAGE_SIDE {
        return Err(format!("{width} x {height}: pictures are 1 to {} pixels a side", crate::hud::MAX_IMAGE_SIDE));
    }
    let data = &dds[data_at..];
    let (w, h) = (width as usize, height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    if matches!(format, Format::Rgba | Format::Bgra) {
        let pixels = data.get(..w * h * 4).ok_or("a DDS texture cut short")?;
        rgba.copy_from_slice(pixels);
        if format == Format::Bgra {
            rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p.swap(0, 2));
        }
        return Ok((width, height, rgba));
    }
    let (blocks_w, blocks_h) = (w.div_ceil(4), h.div_ceil(4));
    let size = format.block_bytes();
    if data.len() < blocks_w * blocks_h * size {
        return Err("a DDS texture cut short".into());
    }
    let mut block = [0u8; 64];
    for by in 0..blocks_h {
        for bx in 0..blocks_w {
            let at = (by * blocks_w + bx) * size;
            let src = &data[at..at + size];
            match format {
                Format::Bc1 => bcdec_rs::bc1(src, &mut block, 16),
                Format::Bc2 => bcdec_rs::bc2(src, &mut block, 16),
                Format::Bc3 => bcdec_rs::bc3(src, &mut block, 16),
                Format::Bc7 => bcdec_rs::bc7(src, &mut block, 16),
                Format::Rgba | Format::Bgra => unreachable!(),
            }
            // The block's rows into the picture (blocks at the right and bottom edges are cut).
            for row in 0..4 {
                let y = by * 4 + row;
                if y >= h {
                    break;
                }
                let cols = (w - bx * 4).min(4);
                let dst = (y * w + bx * 4) * 4;
                rgba[dst..dst + cols * 4].copy_from_slice(&block[row * 16..row * 16 + cols * 4]);
            }
        }
    }
    Ok((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32, code: &[u8; 4], flags: u32) -> Vec<u8> {
        let mut d = vec![0u8; 128];
        d[..4].copy_from_slice(b"DDS ");
        d[4..8].copy_from_slice(&124u32.to_le_bytes());
        d[12..16].copy_from_slice(&height.to_le_bytes());
        d[16..20].copy_from_slice(&width.to_le_bytes());
        d[80..84].copy_from_slice(&flags.to_le_bytes());
        d[84..88].copy_from_slice(code);
        d
    }

    #[test]
    fn a_bc1_block_of_one_colour() {
        // BC1: two 5:6:5 colours and 2-bit indices; colour 0 pure red, every index 0.
        let mut dds = header(4, 4, b"DXT1", 0x4);
        dds.extend_from_slice(&[0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0]);
        let (w, h, rgba) = decode(&dds).unwrap();
        assert_eq!((w, h), (4, 4));
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| p == &[255, 0, 0, 255]), "{:?}", &rgba[..8]);
    }

    #[test]
    fn edge_blocks_are_cut_to_the_picture() {
        // 6 x 5: two blocks across, two down, the right and bottom ones partly outside.
        let mut dds = header(6, 5, b"DXT1", 0x4);
        for _ in 0..4 {
            dds.extend_from_slice(&[0x1F, 0x00, 0x00, 0x00, 0, 0, 0, 0]);
        }
        let (_, _, rgba) = decode(&dds).unwrap();
        assert_eq!(rgba.len(), 6 * 5 * 4);
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| p == &[0, 0, 255, 255]));
    }

    #[test]
    fn dx10_bc7_and_uncompressed_bgra() {
        let mut dds = header(4, 4, b"DX10", 0x4);
        dds.extend_from_slice(&98u32.to_le_bytes());
        dds.extend_from_slice(&[0u8; 16]);
        // Mode 6 (bit 6 set), its endpoints and indices all zero but alpha: a block, decoded.
        let mut block = [0u8; 16];
        block[0] = 0x40;
        dds.extend_from_slice(&block);
        let (w, _, rgba) = decode(&dds).unwrap();
        assert_eq!((w, rgba.len()), (4, 64));

        let mut bgra = header(1, 1, b"\0\0\0\0", 0x41);
        bgra[92..96].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
        bgra.extend_from_slice(&[10, 20, 30, 40]);
        assert_eq!(decode(&bgra).unwrap().2, vec![30, 20, 10, 40]);

        assert!(decode(b"PNG").is_err());
        let mut odd = header(4, 4, b"DX10", 0x4);
        odd.extend_from_slice(&2u32.to_le_bytes());
        odd.extend_from_slice(&[0u8; 16]);
        assert!(decode(&odd).unwrap_err().contains("DXGI format 2"));
    }
}
