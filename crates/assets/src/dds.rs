//! Bounded top-level DDS reader for native DE terrain textures.
use crate::{Error, invalid, sld::bc1_block, slice, u32_at};
const FORMAT: &str = "DDS";
const HEADER: usize = 128;
const MAX_SIDE: usize = 4_096;

/// A decoded RGBA texture, row-major, without alpha premultiplication.
#[derive(Debug, Eq, PartialEq)]
pub struct Texture {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<[u8; 4]>,
}
fn unsupported() -> Error {
    Error::Unsupported {
        format: FORMAT,
        detail: "unsupported DDS texture layout".to_owned(),
    }
}
/// Decode the complete native top level. Reject arrays, volumes and cube maps.
/// Mip generation belongs to the view-resolution importer, never this decoder.
pub fn parse(bytes: &[u8]) -> Result<Texture, Error> {
    if slice(bytes, 0, 4, FORMAT)? != b"DDS " || u32_at(bytes, 4, FORMAT)? != 124 {
        return Err(invalid(FORMAT, 0, "not a DDS header"));
    }
    slice(bytes, 0, HEADER, FORMAT)?;
    let height = u32_at(bytes, 12, FORMAT)? as usize;
    let width = u32_at(bytes, 16, FORMAT)? as usize;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(invalid(FORMAT, 12, "texture dimensions outside limit"));
    }
    if u32_at(bytes, 24, FORMAT)? > 1 || u32_at(bytes, 112, FORMAT)? != 0 {
        return Err(unsupported());
    }
    let fourcc = slice(bytes, 84, 4, FORMAT)?;
    let (header, block_size) = match fourcc {
        b"DXT1" => (HEADER, 8),
        b"DXT5" => (HEADER, 16),
        b"DX10" => {
            slice(bytes, HEADER, 20, FORMAT)?;
            if !matches!(u32_at(bytes, 128, FORMAT)?, 98 | 99)
                || u32_at(bytes, 132, FORMAT)? != 3
                || u32_at(bytes, 136, FORMAT)? != 0
                || u32_at(bytes, 140, FORMAT)? != 1
                || u32_at(bytes, 144, FORMAT)? > 1
            {
                return Err(unsupported());
            }
            (148, 16)
        }
        [0, 0, 0, 0] => return rgba(bytes, width, height),
        _ => return Err(unsupported()),
    };
    if !width.is_multiple_of(4) || !height.is_multiple_of(4) {
        return Err(invalid(FORMAT, 12, "texture is not block aligned"));
    }
    let wide = width / 4;
    let data = slice(bytes, header, wide * (height / 4) * block_size, FORMAT)?;
    let mut pixels = vec![[0; 4]; width * height];
    for (ordinal, raw) in data.chunks_exact(block_size).enumerate() {
        let block = if block_size == 8 {
            let mut b = [0; 8];
            b.copy_from_slice(raw);
            bc1_block(b)
        } else if fourcc == b"DXT5" {
            bc3(raw)
        } else {
            if raw[0] == 0 {
                return Err(invalid(FORMAT, header + ordinal * 16, "invalid BC7 mode"));
            }
            let mut decoded = [0; 16];
            texture2ddecoder::decode_bc7_block(raw, &mut decoded);
            decoded.map(|p| {
                let [b, g, r, a] = p.to_le_bytes();
                [r, g, b, a]
            })
        };
        let (bx, by) = (ordinal % wide * 4, ordinal / wide * 4);
        for (i, pixel) in block.into_iter().enumerate() {
            pixels[(by + i / 4) * width + bx + i % 4] = pixel;
        }
    }
    Ok(Texture {
        width: width as u16,
        height: height as u16,
        pixels,
    })
}
// BC3 always has four colours, including reversed or equal endpoints.
fn bc3(raw: &[u8]) -> [[u8; 4]; 16] {
    let mut colour = [0; 8];
    colour.copy_from_slice(&raw[8..16]);
    let a = u16::from_le_bytes([colour[0], colour[1]]);
    let b = u16::from_le_bytes([colour[2], colour[3]]);
    let mut out = if a == b {
        let mut first = colour;
        first[4..].fill(0);
        [bc1_block(first)[0]; 16]
    } else {
        if a < b {
            colour[0..2].copy_from_slice(&b.to_le_bytes());
            colour[2..4].copy_from_slice(&a.to_le_bytes());
            let bits =
                u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]) ^ 0x55555555;
            colour[4..].copy_from_slice(&bits.to_le_bytes());
        }
        bc1_block(colour)
    };
    let mut alpha = [0; 8];
    alpha.copy_from_slice(&raw[..8]);
    for (pixel, a) in out.iter_mut().zip(crate::sld::bc4_block(alpha)) {
        pixel[3] = a;
    }
    out
}
fn rgba(bytes: &[u8], width: usize, height: usize) -> Result<Texture, Error> {
    if u32_at(bytes, 80, FORMAT)? & 0x40 == 0 || u32_at(bytes, 88, FORMAT)? != 32 {
        return Err(unsupported());
    }
    let masks = [92, 96, 100, 104]
        .map(|at| u32_at(bytes, at, FORMAT))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut used = 0;
    for (i, &mask) in masks.iter().enumerate() {
        if i == 3 && mask == 0 {
            continue;
        }
        if mask.count_ones() != 8 || mask >> mask.trailing_zeros() != 255 || used & mask != 0 {
            return Err(unsupported());
        }
        used |= mask;
    }
    let pitch = u32_at(bytes, 20, FORMAT)? as usize;
    if pitch < width * 4 || pitch > MAX_SIDE * 4 {
        return Err(unsupported());
    }
    let data = slice(bytes, HEADER, pitch * height, FORMAT)?;
    let mut pixels = Vec::with_capacity(width * height);
    for row in data.chunks_exact(pitch) {
        for raw in row[..width * 4].as_chunks::<4>().0 {
            let value = u32::from_le_bytes(*raw);
            pixels.push(std::array::from_fn(|i| {
                if masks[i] == 0 {
                    255
                } else {
                    ((value & masks[i]) >> masks[i].trailing_zeros()) as u8
                }
            }));
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
