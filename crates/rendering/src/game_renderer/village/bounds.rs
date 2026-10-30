use super::*;

/// Visibility from the same selected frames, native hotspots and zoomed bounds
/// used by drawing, including the missing-art marker.
pub fn scene_building_visible(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
) -> bool {
    let mut visible = false;
    village::building(art, unit, camera, animation, &mut |_| visible = true);
    visible
}

/// Hit each displayed native part, using the drawing emitter's animation,
/// offsets, hotspots, elevation and zoom rather than the ground footprint.
pub fn scene_building_hit(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
    point: aoe_core::ScreenPoint,
) -> bool {
    let ndc = [
        point.x / camera.viewport[0] * 2.0 - 1.0,
        1.0 - point.y / camera.viewport[1] * 2.0,
    ];
    let mut hit = false;
    village::building(art, unit, camera, animation, &mut |(sprite, _, _, _)| {
        hit |= (0..2).all(|axis| {
            (ndc[axis] - f64::from(sprite.position[axis])).abs() <= f64::from(sprite.radius[axis])
        });
    });
    hit
}

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
        usize::from(unit.actor_frame() >> if unit.kind.native_directions() { 4 } else { 3 })
            .min(stride - 1)
    } else if animated {
        animation % stride
    } else {
        0
    };
    let (index, mirrored) = unit_frame_index(unit, stride, phase);
    let mut frame = world_frame(*frames.get(index)?);
    if mirrored {
        frame.anchor[0] = frame.size[0] - frame.anchor[0];
    }
    sprite_screen_bounds(frame, unit.position, unit.elevation_meters, camera)
}

/// Exclude packed actor phases that cannot affect visible animated pixels.
pub fn scene_unit_render_key(art: &GameArt, mut unit: SceneUnit, camera: SceneCamera) -> SceneUnit {
    if unit.kind.actor_slot().is_some()
        && !village::frames(art, unit).is_some_and(|(frames, stride, _)| {
            stride > 1 && frames_visible(frames, stride, unit, camera)
        })
    {
        unit.facing &= if unit.kind.native_directions() { 15 } else { 7 };
        unit.visual = unit.visual.with_damage_stage(0);
    }
    unit
}

pub(super) fn frames_visible(
    frames: &[GameFrame],
    stride: usize,
    unit: SceneUnit,
    camera: SceneCamera,
) -> bool {
    let (direction, flipped) = unit_direction(unit);
    frames[direction * stride..(direction + 1) * stride]
        .iter()
        .any(|frame| {
            let mut frame = world_frame(*frame);
            if flipped {
                frame.anchor[0] = frame.size[0] - frame.anchor[0];
            }
            sprite_screen_bounds(frame, unit.position, unit.elevation_meters, camera).is_some()
        })
}

/// Depth by which an object clears the ground it stands on. A frame reaches
/// below its hotspot (a gold pile by half its height), where flat ground is
/// nearer the viewer by one depth unit per 32 pixels; the tallest reviewed
/// overhang is 43 pixels. One shared offset keeps objects ordered among
/// themselves, and a one-level rise in front (two units) still hides them.
const GROUND_CLEARANCE: f64 = 1.5;

/// The rendered body depth, including clearance below the native hotspot.
pub fn scene_unit_depth(unit: SceneUnit) -> f64 {
    object_depth(WorldObject::Unit(unit))
}

pub(super) fn object_depth(object: WorldObject) -> f64 {
    let position = object.position();
    // The native Town Center reaches 64 source pixels below its anchor, farther
    // than a tree/unit. Its front foundation must clear its own flat footprint.
    let clearance = match object {
        WorldObject::Unit(unit) if unit.visual.ghost() => 32.0,
        // Order feedback stays above a building used as the rally target.
        WorldObject::Resource(r, _, _) if r.kind == SceneResource::RALLY_FLAG => 4.0,
        // The deck lies below actors walking across it, above the water.
        WorldObject::Unit(unit)
            if unit
                .kind
                .building_slot()
                .is_some_and(|slot| (65..=70).contains(&slot)) =>
        {
            1.25
        }
        WorldObject::Unit(unit) if unit.kind.building_side().is_some() => 3.0,
        _ => GROUND_CLEARANCE,
    };
    surface_depth([position[0], position[1], object.elevation_meters()]) + clearance
}
