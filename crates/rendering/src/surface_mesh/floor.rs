//! Displayed-vertex field only: four incident source cells, no resident-world map.
use super::*;
#[cfg(test)]
mod fixtures;
#[cfg(test)]
pub(crate) use fixtures::*;

const MISSING: u16 = u16::MAX;

#[inline(never)]
pub(super) fn assign(triangles: &mut [ProjectedSurfaceTriangle], terrain: &[SceneTerrain]) {
    let mut keys = crate::WorldKeyIndex::default();
    let mut values: Vec<[u16; 4]> = Vec::new();
    for triangle in triangles.iter().filter(|t| eligible(t)) {
        for point in triangle.points {
            let index = keys.entry(vertex(point));
            if index == values.len() {
                values.push([MISSING; 4]);
            }
        }
    }
    debug_assert!(values.len() <= MAX_SURFACE_TRIANGLES * 3);
    if values.is_empty() {
        return;
    }
    for sample in terrain {
        let Some(appearance) = sample.appearance else {
            continue;
        };
        if sample.surface.water != 0
            || sample.surface.kind == SceneTerrainSurface::CLIFF
            || !matches!(sample.material, 0 | 1 | 2 | 6)
        {
            continue;
        }
        let tile = tile_key(sample.position);
        for dy in 0..=1 {
            for dx in 0..=1 {
                let (Some(x), Some(y)) = (tile[0].checked_add(dx), tile[1].checked_add(dy)) else {
                    continue;
                };
                if let Some(index) = keys.get([x, y]) {
                    let value = &mut values[index][(dy * 2 + dx) as usize];
                    let floor = appearance.floor_strength.min(1000);
                    // Duplicate source tile records cannot reweight support. Conflicting
                    // duplicates conservatively choose the lower floor, independent of order.
                    *value = (*value).min(floor);
                }
            }
        }
    }
    for support in &mut values {
        let (mut sum, mut count) = (0_u32, 0_u32);
        for &floor in support.iter() {
            if floor != MISSING {
                sum += u32::from(floor);
                count += 1;
            }
        }
        // Compute and quantize once per shared vertex. Missing/legacy/protected
        // samples are not zero observations; no support has a deterministic zero.
        let mean = if count == 0 {
            0
        } else {
            ((sum + count / 2) / count) as u16
        };
        support[0] = u16::from(landscape::quantized_floor(mean));
    }
    for triangle in triangles.iter_mut().filter(|t| eligible(t)) {
        triangle.floor_strengths = Some(triangle.points.map(|point| {
            values[keys.get(vertex(point)).expect("displayed floor vertex")][0] as u8
        }));
    }
}

fn eligible(t: &ProjectedSurfaceTriangle) -> bool {
    t.appearance & 1 != 0 && !t.skirt && t.tint <= 3 && matches!(t.material, 0 | 1 | 2 | 6)
}
fn vertex(point: SurfacePoint) -> [i32; 2] {
    // Mesh corners are integer world coordinates at every texture subdivision/LOD.
    [point.world[0].floor() as i32, point.world[1].floor() as i32]
}
