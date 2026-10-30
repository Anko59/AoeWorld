//! Every DAT terrain texture through one bounded native-cell and mip importer.
use super::*;
use crate::{
    dat::de_survey,
    dds,
    pack::{TerrainSheet, atlas::Planes},
};
use std::collections::BTreeMap;
const CELL: u16 = 256;
pub struct Sheet {
    pub source: String,
    pub path: PathBuf,
    pub hash: String,
    pub size: [u16; 2],
    pub frames: usize,
}
pub struct Catalog {
    pub sheets: Vec<Sheet>,
    pub terrains: Vec<TerrainSheet>,
}
fn sizes(mut size: [u16; 2]) -> Vec<[u16; 2]> {
    let mut out = vec![size];
    while size.iter().copied().max().unwrap_or(0) > 16 {
        size = size.map(|n| (n / 2).max(1));
        out.push(size);
    }
    out
}
fn cells(size: [u16; 2]) -> usize {
    usize::from(size[0].div_ceil(CELL)) * usize::from(size[1].div_ceil(CELL))
}
pub fn resolve(install: &Path, hasher: &mut blake3::Hasher) -> Result<Catalog, Error> {
    let dat = de_survey::inflate(&read_bounded(
        &install.join("resources/_common/dat/empires2_x2_p1.dat"),
    )?)?;
    let records = de_survey::terrains::find(&dat)?;
    let mut by_name = BTreeMap::new();
    let mut terrains = Vec::new();
    for t in records
        .iter()
        .filter(|t| t.dimensions.iter().all(|&n| n > 0))
    {
        let source = format!("de/terrain/{}.dds", t.texture);
        if !by_name.contains_key(&source) {
            let path = install.join(TEXTURES).join(format!("{}.dds", t.texture));
            let bytes = read_bounded(&path)?;
            let hash = blake3::hash(&bytes).to_hex().to_string();
            hasher.update(source.as_bytes());
            hasher.update(hash.as_bytes());
            let texture = dds::parse(&bytes)?;
            let size = [texture.width, texture.height];
            if size
                .iter()
                .any(|&n| !(16..=4096).contains(&n) || !n.is_power_of_two())
            {
                return Err(crate::invalid(
                    "DE terrain",
                    0,
                    "unsupported native dimensions",
                ));
            }
            by_name.insert(
                source.clone(),
                Sheet {
                    source: source.clone(),
                    path,
                    hash,
                    size,
                    frames: sizes(size).into_iter().map(cells).sum(),
                },
            );
        }
        let s = &by_name[&source];
        terrains.push(TerrainSheet {
            id: t.id,
            source,
            period: t.dimensions,
            size: s.size,
        });
    }
    hasher.update(&serde_json::to_vec(&terrains)?);
    Ok(Catalog {
        sheets: by_name.into_values().collect(),
        terrains,
    })
}
pub(crate) struct Decoder {
    levels: Vec<dds::Texture>,
}
impl Decoder {
    pub fn new(texture: dds::Texture) -> Self {
        let mut levels = vec![texture];
        while levels.last().is_some_and(|t| t.width.max(t.height) > 16) {
            let p = &levels[levels.len() - 1];
            let w = (p.width / 2).max(1);
            let h = (p.height / 2).max(1);
            let mut pixels = Vec::with_capacity(usize::from(w) * usize::from(h));
            for y in 0..h {
                for x in 0..w {
                    let mut sum = [0_u32; 4];
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let at = usize::from((y * 2 + dy).min(p.height - 1))
                                * usize::from(p.width)
                                + usize::from((x * 2 + dx).min(p.width - 1));
                            for (i, c) in p.pixels[at].iter().enumerate() {
                                sum[i] += u32::from(*c);
                            }
                        }
                    }
                    pixels.push(sum.map(|n| ((n + 2) / 4) as u8));
                }
            }
            levels.push(dds::Texture {
                width: w,
                height: h,
                pixels,
            });
        }
        Self { levels }
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.levels.iter().map(|t| cells([t.width, t.height])).sum()
    }
    pub fn frame(&self, mut frame: usize) -> Result<Planes, Error> {
        for t in &self.levels {
            let count = cells([t.width, t.height]);
            if frame >= count {
                frame -= count;
                continue;
            }
            let wide = usize::from(t.width.div_ceil(CELL));
            let x = (frame % wide) as u16 * CELL;
            let y = (frame / wide) as u16 * CELL;
            let w = (t.width - x).min(CELL);
            let h = (t.height - y).min(CELL);
            let mut out = Planes::blank(w, h, 0, 0);
            for row in 0..h {
                for col in 0..w {
                    let p = t.pixels
                        [usize::from(y + row) * usize::from(t.width) + usize::from(x + col)];
                    let at = (usize::from(row) * usize::from(w) + usize::from(col)) * 4;
                    out.color[at..at + 4].copy_from_slice(&p);
                }
            }
            return Ok(out);
        }
        Err(crate::invalid(
            "DE terrain",
            frame,
            "cell outside mip chain",
        ))
    }
}
#[cfg(test)]
mod tests;
