//! Bounded reader for the top mip level of DXT1 (BC1) DDS textures.
use crate::{Error, invalid, sld::bc1_block, slice, u32_at};

const FORMAT: &str = "DDS";
const HEADER: usize = 128;
const MAX_SIDE: usize = 4_096;

/// A decoded RGBA texture, row-major.
#[derive(Debug, Eq, PartialEq)]
pub struct Texture {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<[u8; 4]>,
}

/// Decodes the first mip level; other levels and formats are rejected or ignored.
pub fn parse_bc1(bytes: &[u8]) -> Result<Texture, Error> {
    if slice(bytes, 0, 4, FORMAT)? != b"DDS " || u32_at(bytes, 4, FORMAT)? != 124 {
        return Err(invalid(FORMAT, 0, "not a DDS header"));
    }
    let height = u32_at(bytes, 12, FORMAT)? as usize;
    let width = u32_at(bytes, 16, FORMAT)? as usize;
    if slice(bytes, 84, 4, FORMAT)? != b"DXT1" {
        return Err(Error::Unsupported {
            format: FORMAT,
            detail: "only DXT1 textures are supported".to_owned(),
        });
    }
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(invalid(FORMAT, 12, "texture dimensions outside limit"));
    }
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
        return Err(invalid(FORMAT, 12, "texture is not block aligned"));
    }
    let blocks_wide = width / 4;
    let data = slice(bytes, HEADER, blocks_wide * (height / 4) * 8, FORMAT)?;
    let mut pixels = vec![[0u8; 4]; width * height];
    for (ordinal, raw) in data.as_chunks::<8>().0.iter().enumerate() {
        let mut block = [0u8; 8];
        block.copy_from_slice(raw);
        let (bx, by) = (ordinal % blocks_wide * 4, ordinal / blocks_wide * 4);
        for (i, pixel) in bc1_block(block).into_iter().enumerate() {
            pixels[(by + i / 4) * width + bx + i % 4] = pixel;
        }
    }
    Ok(Texture {
        width: width as u16,
        height: height as u16,
        pixels,
    })
}

#[path = "dds/tests.rs"]
#[cfg(test)]
mod tests;
