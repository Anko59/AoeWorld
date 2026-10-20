//! The AoK terrain directory, immediately following the graphics table.
use super::{
    DatFile,
    reader::{Reader, text},
};
use crate::Error;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Terrain {
    pub id: i16,
    pub name: String,
    pub file_name: String,
    pub slp_id: i32,
    pub terrain_to_draw: i16,
    pub dimensions: [u16; 2],
}

pub(super) fn decode(r: &mut Reader) -> Result<Vec<Terrain>, Error> {
    r.skip(24 + 19 * 6 + 2)?; // map header, tile sizes, alignment
    let mut terrains = Vec::with_capacity(32);
    for id in 0..32 {
        let start = r.pos;
        r.skip(2)?;
        let name = text(r.take(13)?);
        let file_name = text(r.take(13)?);
        let slp_id = r.i32()?;
        r.skip(160)?; // pointers, blending, animation and 19 elevation frames
        let terrain_to_draw = r.i16()?;
        let dimensions = [r.u16()?, r.u16()?];
        r.skip(416 - (r.pos - start))?;
        terrains.push(Terrain {
            id,
            name,
            file_name,
            slp_id,
            terrain_to_draw,
            dimensions,
        });
    }
    Ok(terrains)
}

impl DatFile {
    pub fn terrain_slp(&self, id: i16) -> Option<i32> {
        Some(self.terrain_texture(id)?.slp_id)
    }

    pub fn terrain_texture(&self, id: i16) -> Option<&Terrain> {
        let mut id = id;
        for _ in 0..self.terrains.len() {
            let terrain = self.terrains.get(usize::try_from(id).ok()?)?;
            if terrain.terrain_to_draw < 0 || terrain.terrain_to_draw == id {
                return (terrain.slp_id >= 0).then_some(terrain);
            }
            id = terrain.terrain_to_draw;
        }
        None
    }
}
