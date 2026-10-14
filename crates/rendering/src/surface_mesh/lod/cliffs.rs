//! Shared rocky rim/foot presentation, applied after integer material ownership.
use super::*;

pub(super) fn sculpt(
    faces: &mut [ProjectedSurfaceTriangle],
    cells: &[Option<CellSample>],
    grid: CellGrid,
    camera: &Camera,
) {
    if !cells
        .iter()
        .flatten()
        .any(|c| c.sample.surface.kind == SceneTerrainSurface::CLIFF_LEDGE)
    {
        return;
    }
    let width = grid.width + 1;
    let rims = (0..width * (grid.height + 1))
        .map(|index| {
            let vertex = [
                grid.min[0] + (index % width) as i32 * grid.size,
                grid.min[1] + (index / width) as i32 * grid.size,
            ];
            rim(vertex, cells, grid)
        })
        .collect::<Vec<_>>();
    for face in faces {
        let original = face.points.map(|p| p.world);
        for point in &mut face.points {
            let local = [
                (point.world[0] - f64::from(grid.min[0])) / f64::from(grid.size),
                (point.world[1] - f64::from(grid.min[1])) / f64::from(grid.size),
            ];
            let cell = local.map(|n| n.floor() as usize);
            let fraction = [local[0] - cell[0] as f64, local[1] - cell[1] as f64];
            let mut offset = [0.0; 2];
            for (dx, dy, weight) in [
                (0, 0, (1.0 - fraction[0]) * (1.0 - fraction[1])),
                (1, 0, fraction[0] * (1.0 - fraction[1])),
                (1, 1, fraction[0] * fraction[1]),
                (0, 1, (1.0 - fraction[0]) * fraction[1]),
            ] {
                if weight == 0.0 || cell[0] + dx > grid.width || cell[1] + dy > grid.height {
                    continue;
                }
                let Some(rim) = rims[(cell[1] + dy) * width + cell[0] + dx] else {
                    continue;
                };
                for (axis, value) in offset.iter_mut().enumerate() {
                    *value += weight * rim[axis];
                }
            }
            *point = project_world(
                camera,
                [point.world[0] + offset[0], point.world[1] + offset[1]],
                point.world[2],
            );
        }
        if face.pickable
            && face
                .points
                .iter()
                .zip(original)
                .any(|(p, at)| p.world != at)
        {
            let lo = [0, 1].map(|axis| {
                original
                    .iter()
                    .map(|p| p[axis])
                    .fold(f64::INFINITY, f64::min)
            });
            let hi = [0, 1].map(|axis| {
                original
                    .iter()
                    .map(|p| p[axis])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            let span = ((hi[0] - lo[0]).max(hi[1] - lo[1]) as u32).ilog2() as u8;
            let mode = if original[0][0] > lo[0] {
                3
            } else if original[1][1] > lo[1] {
                1
            } else if original[2][0] == lo[0] {
                2
            } else {
                0
            };
            face.order = super::super::SCULPTED_ORDER | (span << 2) | mode;
        }
    }
}

fn rim(vertex: [i32; 2], cells: &[Option<CellSample>], grid: CellGrid) -> Option<[f64; 2]> {
    // Keep a full surrounding ring, including the triangles just outside the
    // four incident cells. A clipped or tiny fixture keeps its exact planes.
    for axis in 0..2 {
        let extent = if axis == 0 { grid.width } else { grid.height } as i32;
        let at = vertex[axis] - grid.min[axis];
        if at < 2 * grid.size || at > (extent - 2) * grid.size {
            return None;
        }
    }
    for dy in -2..=1 {
        for dx in -2..=1 {
            if grid_index(
                grid,
                [vertex[0] + dx * grid.size, vertex[1] + dy * grid.size],
            )
            .and_then(|index| cells[index])
            .is_none()
            {
                return None;
            }
        }
    }
    let offsets = [[-1, -1], [0, -1], [0, 0], [-1, 0]];
    let mut samples = [None; 4];
    for (i, offset) in offsets.iter().enumerate() {
        samples[i] = grid_index(
            grid,
            [
                vertex[0] + offset[0] * grid.size,
                vertex[1] + offset[1] * grid.size,
            ],
        )
        .and_then(|index| cells[index])
        .map(|c| c.sample);
    }
    // Incomplete halo retains its exact contact planes, refined once loaded.
    let [Some(a), Some(b), Some(c), Some(d)] = samples else {
        return None;
    };
    let samples = [a, b, c, d];
    if samples.iter().any(|s| s.surface.water != 0) {
        return None;
    }
    let cliff = samples.map(|s| s.surface.kind == SceneTerrainSurface::CLIFF_LEDGE);
    let count = cliff.iter().filter(|v| **v).count();
    if count == 0 || count == 4 {
        return None;
    }
    let key = super::super::cliff_art::hash(
        (vertex[0] as u32).wrapping_mul(0x9e37_79b9) ^ (vertex[1] as u32).rotate_left(16),
    );
    let noise = |shift: u32| f64::from((key >> shift) & 255_u32) / 255.0 - 0.5;
    let mut jitter = [noise(0) * 0.44, noise(8) * 0.44];
    if count == 1 || count == 3 {
        let minority = cliff.iter().position(|v| *v == (count == 1))?;
        for (axis, value) in jitter[..2].iter_mut().enumerate() {
            *value += if offsets[minority][axis] == -1 {
                -0.5
            } else {
                0.5
            };
        }
    }
    // A cosmetic corner must stay outside the central walkable interior of
    // every incident source tile, including a three-ledge concave corner.
    Some(jitter.map(|offset| offset.clamp(-0.24, 0.24)))
}
