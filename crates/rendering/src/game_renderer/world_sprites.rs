use super::*;
use crate::resource_frame_index;
use crate::terrain::visible_terrain_frames;

/// Only approved broadleaf frames exist. New ecological families explicitly
/// fall back to healthy broadleaf art; no family name promotes another sheet.
fn scene_resource_index(resource: SceneResource, count: usize) -> Option<usize> {
    if resource.kind == 1 && resource.visual_family != 0 && count >= 14 {
        const HEALTHY: [usize; 11] = [0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13];
        Some(HEALTHY[usize::from(resource.visual_variant) % HEALTHY.len()])
    } else {
        resource_frame_index(resource.kind, resource.visual_variant, count)
    }
}

pub fn scene_resource_frame(art: &GameArt, resource: SceneResource) -> Option<GameFrame> {
    let frames = art.resources.get(usize::from(resource.kind))?;
    frames
        .get(scene_resource_index(resource, frames.len())?)
        .copied()
}

pub(super) fn world_sprite_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(Sprite, GameFrame, f64, u64)> {
    let mut result = if terrain.is_empty() {
        visible_terrain_frames(art, terrain, camera)
            .into_iter()
            .map(|(sprite, frame)| (sprite, frame, f64::NEG_INFINITY, 0))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let mut objects = Vec::new();
    for resource in resources {
        let Some(frames) = art.resources.get(usize::from(resource.kind)) else {
            continue;
        };
        if frames.is_empty() {
            continue;
        }
        let Some(frame_index) = scene_resource_index(*resource, frames.len()) else {
            continue;
        };
        let Some(frame) = frames.get(frame_index) else {
            continue;
        };
        let shadow = (resource.kind == 1)
            .then(|| art.tree_shadows.get(frame_index).copied())
            .flatten();
        objects.push(WorldObject::Resource(*resource, *frame, shadow));
    }
    objects.extend(units.iter().copied().map(WorldObject::Unit));

    for object in objects {
        let WorldObject::Unit(unit) = object else {
            let WorldObject::Resource(resource, frame, paired_shadow) = object else {
                continue;
            };
            let depth = object_depth(object);
            if let Some(shadow_frame) = paired_shadow {
                if let Some(sprite) = scene_sprite(
                    shadow_frame,
                    resource.position,
                    resource.elevation_meters,
                    camera,
                ) {
                    result.push((
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
                result.push((sprite, shadow_frame, depth, object.stable_id()));
            }
            if let Some(sprite) =
                scene_sprite(frame, resource.position, resource.elevation_meters, camera)
            {
                result.push((
                    sprite,
                    scaled(frame, camera.zoom as f32),
                    depth,
                    object.stable_id(),
                ));
            }
            continue;
        };
        let screen = projection.world_to_screen_at_height(unit.position, unit.elevation_meters);
        let frames = if unit.moving {
            &art.walking
        } else {
            &art.standing
        };
        if frames.is_empty() {
            continue;
        }
        let (direction, flipped) = sprite_direction(unit.facing);
        let frame = frames[direction * 10 + if unit.moving { animation % 10 } else { 0 }];
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
        };
        let mut scaled_frame = frame;
        scaled_frame.size = scaled_frame.size.map(|value| value * scale);
        scaled_frame.anchor = scaled_frame.anchor.map(|value| value * scale);
        let depth = object_depth(object);
        if let Some((shadow, shadow_frame)) =
            alpha_shadow(frame, unit.position, unit.elevation_meters, camera, 0.28)
        {
            result.push((shadow, shadow_frame, depth, object.stable_id()));
        }
        result.push((sprite, scaled_frame, depth, object.stable_id()));
    }
    result
}

fn object_depth(object: WorldObject) -> f64 {
    let position = object.position();
    surface_depth([position[0], position[1], object.elevation_meters()])
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
    sprite_screen_bounds(frame, resource.position, resource.elevation_meters, camera)
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
#[wasm_bindgen_test::wasm_bindgen_test]
fn composed_layer_sort_matches_two_stable_sorts_on_exact_depth_and_id_ties() {
    use bytemuck::Zeroable;
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [0.0; 4],
        },
        size: [1.0; 2],
        anchor: [0.0; 2],
    };
    let camera = SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    };
    let objects = (0..96)
        .map(|index| {
            let mut sprite = Sprite::zeroed();
            sprite.color[0] = index as f32;
            let depth = [0.0, -0.0, 4.0, -1.0, f64::INFINITY, f64::NEG_INFINITY][index % 6];
            (sprite, frame, depth, (index % 4) as u64)
        })
        .collect::<Vec<_>>();
    // Original object pre-sort, followed by the old stable layer depth sort.
    let mut expected = objects.clone();
    expected.sort_by(|left, right| left.2.total_cmp(&right.2).then(left.3.cmp(&right.3)));
    expected.sort_by(|left, right| left.2.total_cmp(&right.2));
    let actual = ordered_world_layers([], objects, &[], camera);
    let actual = actual
        .iter()
        .map(|layer| {
            let WorldLayer::Sprite(sprite, _, _, _) = layer else {
                panic!("unexpected layer")
            };
            sprite.color[0]
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|entry| entry.0.color[0])
            .collect::<Vec<_>>()
    );
}

fn sprite_direction(facing: u8) -> (usize, bool) {
    let facing = facing % 8;
    let row = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(facing)];
    (row, matches!(facing, 1..=3))
}
