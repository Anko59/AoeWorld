//! Farm ground textures (`g_fm1`, `g_fm2`): DE draws farms with terrain art, no sprite.
use super::FARM_TEXTURES;
use crate::{
    Error, dds,
    pack::atlas::{Planes, SpriteRef},
};
use std::{fs, path::Path};

pub(super) struct Sheet {
    pub name: &'static str,
    pub hash: String,
    pub texture: dds::Texture,
}

pub(super) fn load(dir: &Path, hasher: &mut blake3::Hasher) -> Result<Vec<Sheet>, Error> {
    let mut sheets = Vec::new();
    for name in FARM_TEXTURES {
        let path = dir.join(format!("{name}.dds"));
        if !path.is_file() {
            continue;
        }
        if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
            return Err(crate::invalid("DE", 0, format!("{name}.dds is too large")));
        }
        let bytes = fs::read(&path)?;
        let hash = blake3::hash(&bytes);
        hasher.update(name.as_bytes());
        hasher.update(hash.as_bytes());
        sheets.push(Sheet {
            name,
            hash: hash.to_hex().to_string(),
            texture: dds::parse_bc1(&bytes)?,
        });
    }
    Ok(sheets)
}

impl Sheet {
    pub(super) fn sprite(&self) -> SpriteRef {
        SpriteRef {
            id: format!("de/{}.dds", self.name),
            hash: self.hash.clone(),
            frames: 1,
        }
    }

    /// A quarter-size RGB overview of the whole sheet.
    pub(super) fn preview(&self) -> (u32, u32, Vec<u8>) {
        let (w, h) = (
            usize::from(self.texture.width),
            usize::from(self.texture.height),
        );
        let (pw, ph) = (w / 4, h / 4);
        let mut rgb = Vec::with_capacity(pw * ph * 3);
        for y in 0..ph {
            for x in 0..pw {
                rgb.extend_from_slice(&self.texture.pixels[y * 4 * w + x * 4][..3]);
            }
        }
        (pw as u32, ph as u32, rgb)
    }

    /// The sheet at x1 scale: DE ships terrain at 2x, so each 2x2 texel block
    /// is averaged. The result is one opaque, seamless 1:1 ground texture that the
    /// client samples by world position (it is not a diamond sprite).
    pub(super) fn planes(&self, _frame: usize) -> Result<Planes, Error> {
        let (w, h) = (
            usize::from(self.texture.width) / 2,
            usize::from(self.texture.height) / 2,
        );
        let mut planes = Planes::blank(w as u16, h as u16, 0, 0);
        let full = usize::from(self.texture.width);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.texture.pixels[(y * 2 + dy) * full + x * 2 + dx];
                    for k in 0..3 {
                        sum[k] += u32::from(p[k]);
                    }
                }
                let at = (y * w + x) * 4;
                planes.color[at..at + 4].copy_from_slice(&[
                    (sum[0] / 4) as u8,
                    (sum[1] / 4) as u8,
                    (sum[2] / 4) as u8,
                    255,
                ]);
            }
        }
        Ok(planes)
    }
}
