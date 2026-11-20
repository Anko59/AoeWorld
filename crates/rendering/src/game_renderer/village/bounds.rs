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
    let (index, mirrored) = unit_frame_index(unit, stride, phase, frames.len() / stride);
    let mut frame = world_frame(*frames.get(index)?);
    if mirrored {
        frame.anchor[0] = frame.size[0] - frame.anchor[0];
    }
    sprite_screen_bounds(
        frame,
        unit.position,
        f64::from(unit.elevation_meters),
        camera,
    )
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
    let (direction, flipped) = unit_direction(unit, frames.len() / stride);
    frames[direction * stride..(direction + 1) * stride]
        .iter()
        .any(|frame| {
            let mut frame = world_frame(*frame);
            if flipped {
                frame.anchor[0] = frame.size[0] - frame.anchor[0];
            }
            sprite_screen_bounds(
                frame,
                unit.position,
                f64::from(unit.elevation_meters),
                camera,
            )
            .is_some()
        })
}

/// Shared minimum depth above the ground. Native actor frames that extend
/// farther below their hotspot receive additional clearance at emission.
/// Ordinary frames retain their relative ordering and one-level cliff hiding.
const GROUND_CLEARANCE: f64 = 1.5;
/// Corpses start flatter than standing actors; the frame below the hotspot
/// still lifts a tall one clear of the ground (`frame_clearance`).
pub(super) const CORPSE_CLEARANCE: f64 = 0.5;
/// Most depth a corpse reaches above its ground: native frames stay below two
/// depth units so a one-level rise in front of them still hides them.
pub(super) const CORPSE_CEILING: f64 = 2.0;

const FOOTPRINT_MARGIN: f64 = 0.01;

pub(super) fn building_half_extent(unit: SceneUnit, art: Option<&crate::BuildingArt>) -> [f64; 2] {
    let side = unit.kind.building_side().unwrap_or(1.0);
    art.map_or([side / 2.0; 2], |art| {
        let [x, y] = art.footprint.map(|value| f64::from(value) / 2.0);
        if matches!(art.unit_id, 792 | 796) {
            [y, x]
        } else {
            [x, y]
        }
    })
}

pub(super) fn bridge_deck(unit: SceneUnit) -> bool {
    unit.kind
        .building_slot()
        .is_some_and(|slot| (65..=70).contains(&slot))
}

pub(super) fn footprint_depth(position: [f64; 2], elevation: f64, half: [f64; 2]) -> f64 {
    surface_depth([position[0] + half[0], position[1] + half[1], elevation]) + FOOTPRINT_MARGIN
}

pub(super) fn building_depth(unit: SceneUnit, art: Option<&crate::BuildingArt>) -> f64 {
    if unit.visual.ghost() || bridge_deck(unit) {
        let clearance = if unit.visual.ghost() {
            32.0
        } else {
            super::decks::DECK_CLEARANCE
        };
        surface_depth([
            unit.position[0],
            unit.position[1],
            f64::from(unit.elevation_meters),
        ]) + clearance
    } else {
        footprint_depth(
            unit.position,
            f64::from(unit.elevation_meters),
            building_half_extent(unit, art),
        )
    }
}

// Project the measured DE 623 ground rim onto the native 96×48 tile diamond.
pub(super) const ROCK_HALF_EXTENT: [f64; 2] = [100.0 / 96.0, 86.0 / 96.0];

/// Static body depth used by hit testing before comparing against terrain.
pub fn scene_unit_depth(art: &GameArt, unit: SceneUnit) -> f64 {
    if unit.kind.building_side().is_some() {
        let id = unit.kind.oriented_building_id(unit.facing);
        return building_depth(
            unit,
            art.compositions.iter().find(|b| Some(b.unit_id) == id),
        );
    }
    object_depth(WorldObject::Unit(unit))
}

pub(super) fn object_depth(object: WorldObject) -> f64 {
    let position = object.position();
    let clearance = match object {
        // Order feedback stays above a building used as the rally target.
        WorldObject::Resource(r, _, _) if r.kind == SceneResource::RALLY_FLAG => 4.0,
        WorldObject::Resource(r, _, _) if r.kind == SceneResource::ROCK => {
            return footprint_depth(position, r.elevation_meters, ROCK_HALF_EXTENT);
        }
        WorldObject::Unit(unit) if unit.kind.building_side().is_some() => {
            return building_depth(unit, None);
        }
        WorldObject::Unit(unit) if unit.kind.is_corpse() => CORPSE_CLEARANCE,
        _ => GROUND_CLEARANCE,
    };
    surface_depth([position[0], position[1], object.elevation_meters()]) + clearance
}

pub(super) fn animates(art: &GameArt, unit: SceneUnit, camera: SceneCamera) -> bool {
    if let Some(slot @ (4 | 40)) = unit.kind.building_slot()
        && !art
            .compositions
            .iter()
            .any(|b| Some(b.unit_id) == unit.kind.building_id() && !b.parts.is_empty())
    {
        let frames = &art.buildings[usize::from(slot)];
        return (slot == 40 || frames.len() == 44)
            && frames.iter().any(|f| {
                sprite_screen_bounds(
                    world_frame(*f),
                    unit.position,
                    f64::from(unit.elevation_meters),
                    camera,
                )
                .is_some()
            });
    }
    if unit.visual.moving() && unit.kind.building_side().is_some() {
        return art
            .compositions
            .iter()
            .find(|b| Some(b.unit_id) == unit.kind.oriented_building_id(unit.facing))
            .is_some_and(|b| {
                let elapsed = art
                    .building_clocks
                    .iter()
                    .find(|(id, _)| *id == unit.id.0)
                    .map_or(0.0, |(_, ms)| *ms);
                elapsed < f64::from(b.destruction_millis())
                    && b.states.iter().any(|(role, parts)| {
                        *role == 0
                            && parts
                                .iter()
                                .any(|p| p.frame_rate > 0.0 && p.animation.len() > 1)
                    })
                    && scene_building_visible(art, unit, camera, 0)
            });
    }
    if unit.kind.is_foundation() {
        return false;
    }
    if unit.kind.building_id().is_some() {
        return art
            .compositions
            .iter()
            .find(|b| Some(b.unit_id) == unit.kind.oriented_building_id(unit.facing))
            .is_some_and(|b| {
                // A healthy wall draws frame 0 of each part; only damage animates.
                let wall = matches!(
                    unit.kind,
                    SceneUnitKind::PalisadeWall | SceneUnitKind::StoneWall
                );
                let parts: &[_] = if wall { &[] } else { &b.parts };
                let damage = usize::from(unit.visual.damage_stage())
                    .checked_sub(1)
                    .and_then(|i| b.damage.get(i))
                    .into_iter()
                    .flat_map(|d| &d.parts);
                parts.iter().chain(damage).any(|p| {
                    p.frame_rate > 0.0
                        && p.animation.len() > 1
                        && scene_sprite(
                            world_frame(p.frame),
                            unit.position,
                            f64::from(unit.elevation_meters),
                            camera,
                        )
                        .is_some()
                })
            });
    }
    let Some((frames, stride, true)) = village::frames(art, unit) else {
        return false;
    };
    frames_visible(frames, stride, unit, camera)
}
