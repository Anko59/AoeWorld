use super::*;
use crate::resource_frame_index;
use crate::terrain::visible_terrain_frames;

#[path = "village/fish.rs"]
mod fish;
#[path = "scenery.rs"]
mod scenery;
#[path = "village/mod.rs"]
mod village;
pub use fish::scene_resource_animation_frame;

#[path = "village/bounds.rs"]
mod bounds;
use bounds::object_depth;
pub use bounds::{
    scene_building_hit, scene_building_visible, scene_unit_bounds, scene_unit_depth,
    scene_unit_render_key,
};

/// Fallback broadleaf selection retains its paired shadow index.
fn scene_resource_index(resource: SceneResource, count: usize) -> Option<usize> {
    if resource.kind == 1 && resource.visual_family != 0 && count >= 14 {
        // The broadleaf table's first eleven entries are the reviewed healthy set.
        resource_frame_index(1, resource.visual_variant % 11, count)
    } else {
        resource_frame_index(resource.kind, resource.visual_variant, count)
    }
}

pub fn scene_resource_presentation(
    art: &GameArt,
    resource: SceneResource,
) -> Option<(GameFrame, Option<GameFrame>)> {
    resource_presentation_at(art, resource, 0)
}

pub(super) fn resource_presentation_at(
    art: &GameArt,
    resource: SceneResource,
    animation: usize,
) -> Option<(GameFrame, Option<GameFrame>)> {
    if resource.kind == 0 && matches!(resource.visual_family, 5 | 6) {
        return fish::frame(art, resource, animation).map(|f| (f, None));
    }
    if resource.kind >= SceneResource::CLIFF
        || matches!(
            resource.kind,
            SceneResource::CHOPPED_TREE | SceneResource::TREE_STUMP
        )
    {
        return scenery::frame(art, resource).map(|frame| (frame, None));
    }
    if resource.kind == 1 {
        let family = match resource.visual_family {
            2 => Some((&art.tree_families[0], 9, &[1_u8, 2, 3, 4, 7, 8][..])),
            4 => Some((
                &art.tree_families[1],
                13,
                &[0_u8, 1, 2, 3, 5, 6, 8, 10, 11, 12][..],
            )),
            _ => None,
        };
        // A missing or incomplete reviewed group falls back to broadleaf art.
        if let Some((frames, expected, approved)) = family {
            if frames.len() == expected {
                let index = approved[usize::from(resource.visual_variant) % approved.len()];
                return frames
                    .get(usize::from(index))
                    .copied()
                    .map(|frame| (frame, None));
            }
        }
    }
    let frames = art.resources.get(usize::from(resource.kind))?;
    let index = scene_resource_index(resource, frames.len())?;
    Some((
        *frames.get(index)?,
        if resource.kind == 1 {
            art.tree_shadows.get(index).copied()
        } else {
            None
        },
    ))
}

pub fn scene_resource_frame(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    scene_resource_presentation(art, resource).map(|(frame, _)| frame)
}

#[cfg(test)]
pub(super) fn world_sprite_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(Sprite, GameFrame, f64, u64)> {
    let mut result = Vec::new();
    emit_world_sprite_frames(art, terrain, resources, units, camera, animation, |entry| {
        result.push(entry);
    });
    result
}

// Keep the shared object pass out of the large backend packet builders.
#[inline(never)]
pub(super) fn emit_world_sprite_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
    mut emit: impl FnMut((Sprite, GameFrame, f64, u64)),
) {
    if terrain.is_empty() {
        // Keep the bounded diagnostic terrain-frame helper, without collecting another
        // object scene. Surfaces and selections have already been appended.
        for (sprite, frame) in visible_terrain_frames(art, terrain, camera) {
            emit((sprite, frame, f64::NEG_INFINITY, 0));
        }
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    // Preserve resource-before-unit submission without allocating/copying an
    // unsorted intermediate object vector on every frame.
    let objects = resources
        .iter()
        .filter_map(|resource| {
            resource_presentation_at(art, *resource, animation).map(|(frame, shadow)| {
                WorldObject::Resource(*resource, world_frame(frame), shadow.map(world_frame))
            })
        })
        .chain(units.iter().copied().map(WorldObject::Unit));

    for object in objects {
        let WorldObject::Unit(unit) = object else {
            let WorldObject::Resource(resource, frame, paired_shadow) = object else {
                continue;
            };
            let depth = object_depth(object);
            if resource.kind == SceneResource::RALLY_FLAG && art.scenery.rally_flags.is_empty() {
                scenery::emit_flag(resource, camera, depth, &mut emit);
                continue;
            }
            if let Some(shadow_frame) = paired_shadow {
                if let Some(sprite) = scene_sprite(
                    shadow_frame,
                    resource.position,
                    resource.elevation_meters,
                    camera,
                ) {
                    emit((
                        sprite,
                        scaled(shadow_frame, camera.zoom as f32),
                        depth,
                        object.stable_id(),
                    ));
                }
            } else if let Some((sprite, shadow_frame)) = alpha_shadow(
                frame,
                resource.position,
                resource.elevation_meters,
                camera,
                0.2,
            ) {
                emit((sprite, shadow_frame, depth, object.stable_id()));
            }
            if let Some(mut sprite) =
                scene_sprite(frame, resource.position, resource.elevation_meters, camera)
            {
                if resource.kind <= SceneResource::CHOPPED_TREE
                    && !(resource.kind == 0 && matches!(resource.visual_family, 5 | 6))
                {
                    sprite.pages[3] |= Sprite::OCCLUDER;
                }
                sprite.pages[3] |= u32::from(resource.highlight & 3) << 4;
                emit((
                    sprite,
                    scaled(frame, camera.zoom as f32),
                    depth,
                    object.stable_id(),
                ));
            }
            continue;
        };
        let screen = projection.world_to_screen_at_height(unit.position, unit.elevation_meters);
        if unit.kind.building_side().is_some() {
            village::building(art, unit, camera, animation, &mut |(
                mut sprite,
                frame,
                depth,
                id,
            )| {
                if unit.visual.ghost() {
                    sprite.color = if unit.visual.invalid_ghost() {
                        [3.0, 0.2, 0.2, 0.5]
                    } else {
                        [1.0, 1.0, 1.0, 0.5]
                    };
                } else if unit.kind.building_id().is_some_and(|id| id != 50) {
                    sprite.pages[3] |= Sprite::OCCLUDER;
                }
                emit((sprite, frame, depth, id));
            });
            continue;
        }
        let Some((frames, stride, animated)) = village::frames(art, unit) else {
            village::marker(unit, camera, &mut emit);
            continue;
        };
        let (direction, flipped) = sprite_direction(unit.facing);
        let phase = if unit.kind.actor_slot().is_some() {
            usize::from(unit.facing >> 3).min(stride - 1)
        } else if animated {
            let clock = if unit.kind == crate::SceneUnitKind::VillagerFishing {
                animation / 8
            } else {
                animation
            };
            clock % stride
        } else {
            0
        };
        let (index, _) = actor_frame_index(unit.facing, stride, phase);
        let frame = world_frame(frames[index]);
        // The DAT's fishing task is the net delta over the standing hull.
        let net = (unit.kind == crate::SceneUnitKind::FishingShipWorking)
            .then(|| art.fishing[9].get(direction).copied())
            .flatten()
            .map(world_frame);
        // Siege bodies are DAT deltas over the wheels, rails or log base row.
        let mut shadow_emitted = false;
        for frame in std::iter::once(frame)
            .chain(net)
            .chain(village::siege_body_frame(art, unit))
        {
            let scale = camera.zoom as f32;
            let width = f64::from(frame.size[0]) * camera.zoom;
            let height = f64::from(frame.size[1]) * camera.zoom;
            if screen.x + width < 0.0
                || screen.y + height < 0.0
                || screen.x - width > camera.viewport[0]
                || screen.y - height > camera.viewport[1]
            {
                continue;
            }
            let mut uv = frame.atlas.uv;
            if flipped {
                uv[0] += uv[2];
                uv[2] = -uv[2];
            }
            let x = screen.x
                - if flipped {
                    width - f64::from(frame.anchor[0]) * camera.zoom
                } else {
                    f64::from(frame.anchor[0]) * camera.zoom
                };
            let y = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
            let sprite = Sprite {
                position: [
                    ((x + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
                    (1.0 - (y + height / 2.0) / camera.viewport[1] * 2.0) as f32,
                ],
                radius: [
                    (width / camera.viewport[0]) as f32,
                    (height / camera.viewport[1]) as f32,
                ],
                color: [1.0; 4],
                uv,
                depths: [0.0; 4],
                terrain_blend: [[0.0; 4]; 2],
                pages: [frame.atlas.page, 0, 0, 0],
            }
            .with_owner(unit.visual);
            let mut scaled_frame = frame;
            scaled_frame.size = scaled_frame.size.map(|value| value * scale);
            scaled_frame.anchor = scaled_frame.anchor.map(|value| value * scale);
            let depth = object_depth(object);
            if !shadow_emitted
                && let Some((shadow, shadow_frame)) =
                    alpha_shadow(frame, unit.position, unit.elevation_meters, camera, 0.28)
            {
                shadow_emitted = true;
                emit((shadow, shadow_frame, depth, object.stable_id()));
            }
            emit((sprite, scaled_frame, depth, object.stable_id()));
        }
    }
}

/// Art frames are authored for the original 96 by 48 pixel tile diamond, so
/// they grow with the terrain to this world's `ISO_TILE_WIDTH`.
fn world_frame(frame: GameFrame) -> GameFrame {
    scaled(
        frame,
        aoe_core::ISO_TILE_WIDTH as f32 / crate::terrain::TERRAIN_NATIVE_DIAMOND_WIDTH,
    )
}

#[derive(Clone, Copy)]
enum WorldObject {
    Resource(SceneResource, GameFrame, Option<GameFrame>),
    Unit(SceneUnit),
}

impl WorldObject {
    fn position(self) -> [f64; 2] {
        match self {
            Self::Resource(resource, _, _) => resource.position,
            Self::Unit(unit) => unit.position,
        }
    }

    fn stable_id(self) -> u64 {
        match self {
            Self::Resource(resource, _, _) => resource.id,
            Self::Unit(unit) => u64::from(unit.id.0),
        }
    }

    fn elevation_meters(self) -> f64 {
        match self {
            Self::Resource(resource, _, _) => resource.elevation_meters,
            Self::Unit(unit) => unit.elevation_meters,
        }
    }
}

fn scene_sprite(
    frame: GameFrame,
    position: [f64; 2],
    elevation_meters: f64,
    camera: SceneCamera,
) -> Option<Sprite> {
    let bounds = sprite_screen_bounds(frame, position, elevation_meters, camera)?;
    let [left, top, right, bottom] = bounds;
    let width = right - left;
    let height = bottom - top;
    Some(Sprite {
        position: [
            ((left + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
            (1.0 - (top + height / 2.0) / camera.viewport[1] * 2.0) as f32,
        ],
        radius: [
            (width / camera.viewport[0]) as f32,
            (height / camera.viewport[1]) as f32,
        ],
        color: [1.0; 4],
        uv: frame.atlas.uv,
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [frame.atlas.page, 0, 0, 0],
    })
}

fn alpha_shadow(
    frame: GameFrame,
    position: [f64; 2],
    elevation_meters: f64,
    camera: SceneCamera,
    opacity: f32,
) -> Option<(Sprite, GameFrame)> {
    let mut shadow_frame = frame;
    shadow_frame.size[0] *= 0.88;
    shadow_frame.size[1] *= 0.2;
    shadow_frame.anchor[0] *= 0.88;
    shadow_frame.anchor[1] *= 0.2;
    let mut sprite = scene_sprite(shadow_frame, position, elevation_meters, camera)?;
    // Fixed sun: six screen pixels right and four down at unit zoom (NDC × 2).
    sprite.position[0] += (12.0 * camera.zoom / camera.viewport[0]) as f32;
    sprite.position[1] -= (8.0 * camera.zoom / camera.viewport[1]) as f32;
    sprite.color = [0.0, 0.0, 0.0, opacity];
    Some((sprite, scaled(shadow_frame, camera.zoom as f32)))
}

pub(super) fn resource_sprite_bounds(
    resource: SceneResource,
    frame: GameFrame,
    camera: SceneCamera,
) -> Option<[f64; 4]> {
    sprite_screen_bounds(
        world_frame(frame),
        resource.position,
        resource.elevation_meters,
        camera,
    )
}

fn sprite_screen_bounds(
    frame: GameFrame,
    position: [f64; 2],
    elevation_meters: f64,
    camera: SceneCamera,
) -> Option<[f64; 4]> {
    if camera.viewport[0] <= 0.0 || camera.viewport[1] <= 0.0 {
        return None;
    }
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let screen = projection.world_to_screen_at_height(position, elevation_meters);
    let width = f64::from(frame.size[0]) * camera.zoom;
    let height = f64::from(frame.size[1]) * camera.zoom;
    let left = screen.x - f64::from(frame.anchor[0]) * camera.zoom;
    let top = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
    let right = left + width;
    let bottom = top + height;
    if right < 0.0 || bottom < 0.0 || left > camera.viewport[0] || top > camera.viewport[1] {
        return None;
    }
    Some([left, top, right, bottom])
}

fn scaled(mut frame: GameFrame, scale: f32) -> GameFrame {
    frame.size = [frame.size[0] * scale, frame.size[1] * scale];
    frame.anchor = [frame.anchor[0] * scale, frame.anchor[1] * scale];
    frame
}

#[cfg(test)]
#[path = "village/tests/layer_order.rs"]
mod layer_order_tests;

fn sprite_direction(facing: u8) -> (usize, bool) {
    let facing = facing % 8;
    let row = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(facing)];
    (row, matches!(facing, 1..=3))
}

pub fn actor_frame_index(facing: u8, stride: usize, phase: usize) -> (usize, bool) {
    let (direction, mirrored) = sprite_direction(facing);
    (direction * stride + phase % stride.max(1), mirrored)
}

pub(super) fn scene_unit_animates(art: &GameArt, unit: SceneUnit, camera: SceneCamera) -> bool {
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
                    unit.elevation_meters,
                    camera,
                )
                .is_some()
            });
    }
    if unit.kind.is_foundation() {
        return false;
    }
    if unit.kind.building_id().is_some() {
        return art
            .compositions
            .iter()
            .find(|b| Some(b.unit_id) == unit.kind.building_id())
            .is_some_and(|b| {
                let parts = if unit.kind.is_foundation() {
                    &b.construction
                } else {
                    &b.parts
                };
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
                            unit.elevation_meters,
                            camera,
                        )
                        .is_some()
                })
            });
    }
    let Some((frames, stride, true)) = village::frames(art, unit) else {
        return false;
    };
    bounds::frames_visible(frames, stride, unit, camera)
}
