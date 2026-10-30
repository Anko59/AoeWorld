//! Native animation working-set demand, independent of physical atlas placement.
use super::*;

/// Visible native cells, deduplicated independently of their current residency.
pub fn scene_native_frames(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    demand(art, units, camera, animation, 0..=0)
}

/// Two source phases of lookahead, always inside the current facing row.
pub fn scene_native_prefetch(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    demand(art, units, camera, animation, 1..=2)
}

fn demand(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    ahead: std::ops::RangeInclusive<usize>,
) -> Vec<u32> {
    let mut required = std::collections::BTreeSet::new();
    for &unit in units {
        if unit.kind.building_side().is_some() {
            for offset in ahead.clone() {
                village::building(
                    art,
                    unit,
                    camera,
                    animation + offset * 100,
                    &mut |(_, frame, _, _)| {
                        if frame.stream != 0 {
                            required.insert(frame.stream);
                        }
                    },
                );
            }
            continue;
        }
        if unit.kind.actor_slot().is_none() {
            continue;
        }
        let Some([left, top, right, bottom]) = scene_unit_bounds(art, unit, camera, 0) else {
            continue;
        };
        if right < 0.0 || bottom < 0.0 || left > camera.viewport[0] || top > camera.viewport[1] {
            continue;
        }
        let Some((frames, stride, _)) = village::frames(art, unit) else {
            continue;
        };
        let phase =
            usize::from(unit.actor_frame() >> if unit.kind.native_directions() { 4 } else { 3 })
                .min(stride - 1);
        for offset in ahead.clone() {
            let (index, _) = unit_frame_index(
                unit,
                stride,
                (phase + offset) % stride,
                frames.len() / stride,
            );
            if frames[index].stream != 0 {
                required.insert(frames[index].stream);
            }
        }
    }
    required.into_iter().collect()
}

/// Native resource cells share the same visible LRU as units and buildings.
pub fn scene_resource_native_frames(
    art: &GameArt,
    resources: &[SceneResource],
    camera: SceneCamera,
    animation: usize,
) -> Vec<u32> {
    if art.environment.is_empty() {
        return Vec::new();
    }
    let mut required = std::collections::BTreeSet::new();
    for &r in resources {
        environment::frames(art, r, animation, &mut |f| {
            if f.stream != 0 && resource_sprite_bounds(r, f, camera).is_some() {
                required.insert(f.stream);
            }
        });
    }
    required.into_iter().collect()
}
