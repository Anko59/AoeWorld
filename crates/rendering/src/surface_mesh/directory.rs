//! Terrain frame directory: world tile to native atlas frame, shared by every backend.
//!
//! A face that spans several world tiles cannot carry one frame rectangle: each
//! fragment must sample the frame its own world tile owns, at every zoom and
//! mesh LOD. The packer never allocates rows 0–1 of a page (frames start at
//! texel 2), so those rows of page 0 hold a small RGBA8 table read per fragment
//! by WGSL, GLSL and the Canvas rasterizer. Entry `i` is texel `(i % 2048,
//! i / 2048)`; entry 0 stays the white origin. Entries 1..=36 are two header
//! texels per scene material (first frame entry, frame count; sheet columns,
//! rows, zero for coordinate-stable accents), followed by two texels per frame
//! (origin; extent and page). No extra texture, binding or upload exists.
use super::ProjectedSurfaceTriangle;
use super::appearance::{frame_index, terrain_group};
use crate::{AtlasAddress, GAME_ATLAS_BYTES, GAME_ATLAS_PAGES, GAME_ATLAS_SIDE, TerrainTopology};

#[path = "directory/tests/fixture.rs"]
#[cfg(test)]
pub(crate) mod fixture;
#[cfg(test)]
mod tests;

/// Texture mode of a face that spans several world tiles.
pub(crate) const TILED: u8 = 6;
const SIDE: usize = GAME_ATLAS_SIDE as usize;
const ENTRIES: usize = 2 * SIDE;
const MATERIALS: usize = 18;
const FIRST_FRAME: usize = 1 + 2 * MATERIALS;

/// Writes the directory for `art` into the reserved rows of its atlas. Art
/// without terrain frames writes nothing; occupied rows are refused, not overwritten.
pub fn write_terrain_directory(art: &crate::GameArt, atlas: &mut [u8]) -> Result<(), String> {
    if atlas.len() != GAME_ATLAS_BYTES {
        return Err("Invalid game atlas size".into());
    }
    write_rows(art, atlas)
}

/// The same directory written into page 0 alone, for a loader that streams
/// the atlas one page at a time.
pub fn write_terrain_directory_page(art: &crate::GameArt, page: &mut [u8]) -> Result<(), String> {
    if page.len() != crate::GAME_ATLAS_PAGE_BYTES {
        return Err("Invalid game atlas page size".into());
    }
    write_rows(art, page)
}

fn write_rows(art: &crate::GameArt, atlas: &mut [u8]) -> Result<(), String> {
    let mut table = vec![0_u8; (FIRST_FRAME - 1) * 4];
    let mut starts = [0_usize; 18];
    for material in 0..MATERIALS {
        let Some((group, frames, topology)) = terrain_group(art, material as u8) else {
            continue;
        };
        if starts[group] == 0 {
            starts[group] = 1 + table.len() / 4;
            if starts[group] + 2 * frames.len() > ENTRIES {
                return Err("Terrain frame directory overflow".into());
            }
            for frame in frames {
                let [x, y, w, h] = texels(frame.atlas)?;
                table.extend_from_slice(&[
                    x as u8,
                    (x >> 8) as u8,
                    y as u8,
                    (y >> 8) as u8,
                    w as u8,
                    h as u8,
                    ((w >> 8) | (h >> 8 << 4)) as u8,
                    frame.atlas.page as u8,
                ]);
            }
        }
        // Only a sheet the CPU selection accepts is periodic; never frame count alone.
        let (columns, rows) = match topology {
            Some(TerrainTopology::PeriodicXMajorReversedY { columns, rows })
                if topology
                    .and_then(|sheet| sheet.periodic_frame(0, 0, frames.len()))
                    .is_some() =>
            {
                (columns, rows)
            }
            _ => (0, 0),
        };
        for (slot, value) in [starts[group], frames.len()]
            .into_iter()
            .chain([columns, rows].map(usize::from))
            .enumerate()
        {
            table[material * 8 + slot * 2] = value as u8;
            table[material * 8 + slot * 2 + 1] = (value >> 8) as u8;
        }
    }
    if table.len() == (FIRST_FRAME - 1) * 4 {
        return Ok(());
    }
    let target = &mut atlas[4..4 + table.len()];
    if target.iter().any(|byte| *byte != 0) {
        return Err("Atlas rows reserved for the terrain directory are occupied".into());
    }
    target.copy_from_slice(&table);
    Ok(())
}

fn texels(address: AtlasAddress) -> Result<[u32; 4], String> {
    let side = GAME_ATLAS_SIDE as f32;
    let rect = address.uv.map(|value| value * side);
    if address.page >= GAME_ATLAS_PAGES
        || rect
            .iter()
            .any(|value| !(0.0..=side).contains(value) || value.fract() != 0.0)
        || rect[2] < 1.0
        || rect[3] < 1.0
        || rect[0] + rect[2] > side
        || rect[1] + rect[3] > side
    {
        return Err("Terrain frame is not a texel-aligned atlas rectangle".into());
    }
    Ok(rect.map(|value| value as u32))
}

fn entry(atlas: &[u8], index: usize) -> [usize; 4] {
    let start = index * 4;
    [
        atlas[start],
        atlas[start + 1],
        atlas[start + 2],
        atlas[start + 3],
    ]
    .map(usize::from)
}

/// One scene material's frames, as the shaders decode them from the atlas.
#[derive(Clone, Copy)]
pub(crate) struct Sheet {
    start: usize,
    count: usize,
    topology: Option<TerrainTopology>,
}

pub(crate) fn sheet(atlas: &[u8], material: u8) -> Option<Sheet> {
    let material = usize::from(material);
    if atlas.len() != GAME_ATLAS_BYTES || material >= MATERIALS {
        return None;
    }
    let (a, b) = (
        entry(atlas, 1 + 2 * material),
        entry(atlas, 2 + 2 * material),
    );
    let (start, count) = (a[0] | a[1] << 8, a[2] | a[3] << 8);
    if count == 0 || start < FIRST_FRAME || start + 2 * count > ENTRIES {
        return None;
    }
    let (columns, rows) = ((b[0] | b[1] << 8) as u16, (b[2] | b[3] << 8) as u16);
    Some(Sheet {
        start,
        count,
        topology: (columns != 0 && rows != 0)
            .then_some(TerrainTopology::PeriodicXMajorReversedY { columns, rows }),
    })
}

impl Sheet {
    pub(crate) fn frame(self, atlas: &[u8], tile: [i32; 2]) -> AtlasAddress {
        let at = self.start + 2 * frame_index(self.topology, tile, self.count);
        let (a, b) = (entry(atlas, at), entry(atlas, at + 1));
        AtlasAddress {
            page: b[3] as u32,
            uv: [
                a[0] | a[1] << 8,
                a[2] | a[3] << 8,
                b[0] | (b[2] & 15) << 8,
                b[1] | (b[2] >> 4) << 8,
            ]
            .map(|value| value as f32 / GAME_ATLAS_SIDE as f32),
        }
    }
}

/// Corner positions in tile units from the face's texture origin tile.
pub(crate) fn locals(triangle: &ProjectedSurfaceTriangle) -> [[f64; 2]; 3] {
    triangle.points.map(|point| {
        [
            point.world[0] - f64::from(triangle.texture_tile[0]),
            point.world[1] - f64::from(triangle.texture_tile[1]),
        ]
    })
}

/// Tile-local position to the native diamond's frame coordinates.
pub(crate) fn diamond(local: [f64; 2]) -> [f64; 2] {
    [
        0.5 + 0.5 * (local[0] - local[1]),
        0.5 * (local[0] + local[1]),
    ]
}
