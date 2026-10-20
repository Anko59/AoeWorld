use super::*;

/// The actual native body rectangle, shared by selection and order picking.
pub fn scene_unit_bounds(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
) -> Option<[f64; 4]> {
    if unit.kind.building_side().is_some() {
        return None;
    }
    let (frames, stride, animated) = village::frames(art, unit)?;
    let phase = if unit.kind.actor_slot().is_some() {
        usize::from(unit.facing >> 3).min(stride - 1)
    } else if animated {
        animation % stride
    } else {
        0
    };
    let (index, mirrored) = actor_frame_index(unit.facing, stride, phase);
    let mut frame = world_frame(*frames.get(index)?);
    if mirrored {
        frame.anchor[0] = frame.size[0] - frame.anchor[0];
    }
    sprite_screen_bounds(frame, unit.position, unit.elevation_meters, camera)
}
