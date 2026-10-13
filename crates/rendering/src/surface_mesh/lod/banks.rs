//! A quarter-tile corner bevel shared by river and adjacent land faces.
//! No water/ground texture mixing, changed heights or simulation geometry.
use super::*;

pub(super) fn round(
    corners: &mut [SurfacePoint],
    cells: &[Option<CellSample>],
    grid: CellGrid,
    camera: &Camera,
) {
    if grid.size != 1 || cells.is_empty() {
        return;
    }
    for point in corners {
        let vertex = [point.world[0] as i32, point.world[1] as i32];
        let offsets = [[-1, -1], [0, -1], [0, 0], [-1, 0]];
        let mut samples = [None; 4];
        for (i, offset) in offsets.iter().enumerate() {
            samples[i] = grid_index(grid, [vertex[0] + offset[0], vertex[1] + offset[1]])
                .and_then(|index| cells[index])
                .map(|cell| cell.sample);
        }
        // Missing halo, cliffs and unequal heights retain their authoritative
        // edge. A later complete halo can refine this presentation bevel.
        let [Some(a), Some(b), Some(c), Some(d)] = samples else {
            continue;
        };
        let samples = [a, b, c, d];
        if !samples.iter().any(|s| s.surface.water == 3)
            || samples.iter().enumerate().any(|(i, s)| {
                s.surface.kind == SceneTerrainSurface::CLIFF
                    || f64::from(s.surface.corner_game_height_levels[[2, 3, 0, 1][i]])
                        != point.world[2]
            })
        {
            continue;
        }
        let wet = samples.map(|s| s.surface.water != 0);
        let count = wet.iter().filter(|wet| **wet).count();
        // Straight edges and diagonal-only connections stay exact. At a
        // convex/concave corner move toward the minority cell's centre.
        if count != 1 && count != 3 {
            continue;
        }
        let Some(minority) = wet.iter().position(|wet| *wet == (count == 1)) else {
            continue;
        };
        let offset = offsets[minority].map(|offset| if offset == -1 { -0.25 } else { 0.25 });
        *point = project_world(
            camera,
            [point.world[0] + offset[0], point.world[1] + offset[1]],
            point.world[2],
        );
    }
}
