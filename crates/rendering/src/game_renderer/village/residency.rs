//! Native animation working-set demand, independent of physical atlas placement.
use super::*;

/// Visible native cells, deduplicated independently of their current residency.
pub fn scene_native_frames(art: &GameArt, units: &[SceneUnit], camera: SceneCamera) -> Vec<u32> {
    demand(art, units, camera, 0..=0)
}

/// Two source phases of lookahead, always inside the current facing row.
pub fn scene_native_prefetch(art: &GameArt, units: &[SceneUnit], camera: SceneCamera) -> Vec<u32> {
    demand(art, units, camera, 1..=2)
}

fn demand(
    art: &GameArt,
    units: &[SceneUnit],
    camera: SceneCamera,
    ahead: std::ops::RangeInclusive<usize>,
) -> Vec<u32> {
    let mut required = std::collections::BTreeSet::new();
    for &unit in units {
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
