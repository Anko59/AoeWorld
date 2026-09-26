use super::*;
use crate::terrain::visible_terrain_frames;

pub(super) fn world_sprite_frames(
    art: &GameArt,
    terrain: &[SceneTerrain],
    resources: &[SceneResource],
    units: &[SceneUnit],
    camera: SceneCamera,
    animation: usize,
) -> Vec<(Sprite, GameFrame, f64)> {
    let mut result = if terrain.is_empty() {
        visible_terrain_frames(art, terrain, camera)
            .into_iter()
            .map(|(sprite, frame)| (sprite, frame, f64::NEG_INFINITY))
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
        let Some(frame) = frames.get(usize::from(resource.visual_variant) % frames.len()) else {
            continue;
        };
        objects.push(WorldObject::Resource(*resource, *frame));
    }
    objects.extend(units.iter().copied().map(WorldObject::Unit));
    objects.sort_by(|a, b| {
        object_depth(*a)
            .total_cmp(&object_depth(*b))
            .then_with(|| a.stable_id().cmp(&b.stable_id()))
    });
    for object in objects {
        let WorldObject::Unit(unit) = object else {
            let WorldObject::Resource(resource, frame) = object else {
                continue;
            };
            if let Some(sprite) =
                scene_sprite(frame, resource.position, resource.elevation_meters, camera)
            {
                result.push((
                    sprite,
                    scaled(frame, camera.zoom as f32),
                    object_depth(object),
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
        let mut uv = frame.uv;
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
        };
        let mut scaled_frame = frame;
        scaled_frame.size = scaled_frame.size.map(|value| value * scale);
        scaled_frame.anchor = scaled_frame.anchor.map(|value| value * scale);
        result.push((sprite, scaled_frame, object_depth(object)));
    }
    result
}

fn object_depth(object: WorldObject) -> f64 {
    let position = object.position();
    surface_depth([position[0], position[1], object.elevation_meters()])
}

#[derive(Clone, Copy)]
enum WorldObject {
    Resource(SceneResource, GameFrame),
    Unit(SceneUnit),
}

impl WorldObject {
    fn position(self) -> [f64; 2] {
        match self {
            Self::Resource(resource, _) => resource.position,
            Self::Unit(unit) => unit.position,
        }
    }

    fn stable_id(self) -> u64 {
        match self {
            Self::Resource(resource, _) => resource.id,
            Self::Unit(unit) => u64::from(unit.id.0),
        }
    }

    fn elevation_meters(self) -> f64 {
        match self {
            Self::Resource(resource, _) => resource.elevation_meters,
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
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    let screen = projection.world_to_screen_at_height(position, elevation_meters);
    let width = f64::from(frame.size[0]) * camera.zoom;
    let height = f64::from(frame.size[1]) * camera.zoom;
    if screen.x + width < 0.0
        || screen.y + height < 0.0
        || screen.x - width > camera.viewport[0]
        || screen.y - height > camera.viewport[1]
    {
        return None;
    }
    let x = screen.x - f64::from(frame.anchor[0]) * camera.zoom;
    let y = screen.y - f64::from(frame.anchor[1]) * camera.zoom;
    Some(Sprite {
        position: [
            ((x + width / 2.0) / camera.viewport[0] * 2.0 - 1.0) as f32,
            (1.0 - (y + height / 2.0) / camera.viewport[1] * 2.0) as f32,
        ],
        radius: [
            (width / camera.viewport[0]) as f32,
            (height / camera.viewport[1]) as f32,
        ],
        color: [1.0; 4],
        uv: frame.uv,
        depths: [0.0; 4],
    })
}

fn scaled(mut frame: GameFrame, scale: f32) -> GameFrame {
    frame.size = frame.size.map(|value| value * scale);
    frame.anchor = frame.anchor.map(|value| value * scale);
    frame
}

fn sprite_direction(facing: u8) -> (usize, bool) {
    let facing = facing % 8;
    let row = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(facing)];
    (row, matches!(facing, 1..=3))
}
