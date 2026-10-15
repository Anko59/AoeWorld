//! Reviewed native villager rows and the multipart Dark Age Town Center.
use super::*;
use crate::SceneUnitKind;

pub(super) fn frames(art: &GameArt, unit: SceneUnit) -> Option<(&[GameFrame], usize, bool)> {
    let moving = unit.moving || unit.kind == SceneUnitKind::VillagerWorking;
    let (frames, stride) = match unit.kind {
        SceneUnitKind::Scout => (
            if unit.moving {
                &art.walking
            } else {
                &art.standing
            },
            10,
        ),
        SceneUnitKind::VillagerWorking if art.village[2].len() == 75 => (&art.village[2], 15),
        SceneUnitKind::VillagerCarrying if art.village[3].len() == 75 => (&art.village[3], 15),
        _ => (
            if unit.moving {
                &art.village[1]
            } else {
                &art.village[0]
            },
            15,
        ),
    };
    (frames.len() >= stride * 5).then_some((frames, stride, moving))
}

pub(super) fn marker(
    unit: SceneUnit,
    camera: SceneCamera,
    emit: &mut impl FnMut((Sprite, GameFrame, f64, u64)),
) {
    let building = unit.kind == SceneUnitKind::TownCenter;
    let frame = GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [
                0.0,
                0.0,
                1.0 / GAME_ATLAS_SIDE as f32,
                1.0 / GAME_ATLAS_SIDE as f32,
            ],
        },
        size: if building {
            [120.0, 60.0]
        } else {
            [14.0, 32.0]
        },
        anchor: if building { [60.0, 30.0] } else { [7.0, 30.0] },
    };
    if let Some(mut sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
        sprite.color = if building {
            [0.72, 0.58, 0.35, 1.0]
        } else {
            [0.25, 0.57, 0.96, 1.0]
        };
        emit((
            sprite,
            scaled(frame, camera.zoom as f32),
            object_depth(WorldObject::Unit(unit)),
            u64::from(unit.id.0),
        ));
    }
}

pub(super) fn town_center(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    emit: &mut impl FnMut((Sprite, GameFrame, f64, u64)),
) {
    if art.town_center.iter().any(|group| group.len() != 1) {
        marker(unit, camera, emit);
        return;
    }
    // Shadow, rear supports, main structure, front supports, then roofs. All
    // native signed anchors share this world origin; no source pixels are baked.
    for part in [7, 3, 5, 0, 2, 6, 1, 4] {
        let frame = world_frame(art.town_center[part][0]);
        if let Some(sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
            emit((
                sprite,
                scaled(frame, camera.zoom as f32),
                object_depth(WorldObject::Unit(unit)),
                u64::from(unit.id.0),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
