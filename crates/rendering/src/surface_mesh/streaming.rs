//! View-dependent native terrain demand within a fixed part of the shared LRU.
use super::{appearance::terrain_group, *};
use crate::TerrainTopology;
use std::collections::BTreeSet;
const TERRAIN_BYTES: usize = 16 * 1024 * 1024;
fn level(art: &mut GameArt, value: u8) {
    for topology in art
        .terrain_topology
        .iter_mut()
        .chain(art.water_depth.iter_mut().map(|s| &mut s.1))
        .chain(std::iter::once(&mut art.foundation.1))
    {
        if let Some(TerrainTopology::Dds { size, level, .. }) = topology {
            *level = value.min((size[0].max(size[1]).ilog2().saturating_sub(4)) as u8);
        }
    }
}
/// Retain every native level in metadata, request only cells visible at the
/// current projected texel density. Coarsen if the working set exceeds 16 MiB.
pub fn scene_terrain_frames(
    art: &mut GameArt,
    terrain: &[SceneTerrain],
    camera: SceneCamera,
) -> Vec<u32> {
    if !art
        .terrain_topology
        .iter()
        .any(|t| matches!(t, Some(TerrainTopology::Dds { .. })))
    {
        return Vec::new();
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    // DE's native 2x ground has about 205 texels per DAT tile; our tile is 32 px.
    let mut density = 6.4 / camera.zoom.max(0.01);
    let mut initial = 0;
    while initial < 8 && density >= 2.0 {
        density *= 0.5;
        initial += 1;
    }
    for candidate in initial.min(8)..=8 {
        level(art, candidate);
        let groups: [_; 18] = std::array::from_fn(|m| terrain_group(art, m as u8));
        let mut seen = [false; 18];
        let mut directory_cells = 0;
        for (group, frames, _) in groups.iter().flatten() {
            if !seen[*group] {
                seen[*group] = true;
                directory_cells += frames.len();
            }
        }
        let mut required = BTreeSet::new();
        let mut bytes = 0;
        for tile in terrain {
            let screen = projection.world_to_screen_at_height(tile.position, tile.elevation_meters);
            let margin = 128.0 * camera.zoom;
            if screen.x < -margin
                || screen.y < -margin
                || screen.x > camera.viewport[0] + margin
                || screen.y > camera.viewport[1] + margin
            {
                continue;
            }
            // Neighbouring materials participate in blends, including forest beds.
            for material in [tile.material, 0, 2, 6] {
                let Some((_, frames, Some(t @ TerrainTopology::Dds { .. }))) =
                    groups.get(usize::from(material)).copied().flatten()
                else {
                    continue;
                };
                let Some((grid, period)) = t.grid() else {
                    continue;
                };
                let lo = [0, 1].map(|i| {
                    ((tile.position[i] - 1.0) * f64::from(grid[i]) / f64::from(period[i])).floor()
                        as i32
                });
                let hi = [0, 1].map(|i| {
                    ((tile.position[i] + 2.0) * f64::from(grid[i]) / f64::from(period[i])).floor()
                        as i32
                });
                for y in lo[1]..=hi[1] {
                    for x in lo[0]..=hi[0] {
                        let index = y.rem_euclid(i32::from(grid[1])) as usize
                            * usize::from(grid[0])
                            + x.rem_euclid(i32::from(grid[0])) as usize;
                        let frame = frames[index];
                        if frame.stream != 0 && required.insert(frame.stream) {
                            bytes += frame.size[..]
                                .iter()
                                .map(|n| *n as usize + 4)
                                .map(|n| n.div_ceil(64) * 64)
                                .product::<usize>()
                                * 4;
                        }
                    }
                }
            }
        }
        if (bytes <= TERRAIN_BYTES && directory_cells <= directory::MAX_CELLS) || candidate == 8 {
            return required.into_iter().collect();
        }
    }
    Vec::new()
}
