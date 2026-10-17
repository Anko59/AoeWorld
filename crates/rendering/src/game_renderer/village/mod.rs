//! Reviewed native villager rows and the multipart Dark Age Town Center.
use super::*;
use crate::SceneUnitKind;

pub(super) fn frames(art: &GameArt, unit: SceneUnit) -> Option<(&[GameFrame], usize, bool)> {
    let scout = unit.kind == SceneUnitKind::Scout;
    let (frames, stride): (&[GameFrame], usize) = if scout {
        (
            if unit.moving {
                &art.walking
            } else {
                &art.standing
            },
            10,
        )
    } else {
        let group = match unit.kind {
            SceneUnitKind::VillagerChopping => 2,
            SceneUnitKind::VillagerCarryingWood => 3,
            SceneUnitKind::VillagerForaging => 4,
            SceneUnitKind::VillagerCarryingFood => 5,
            SceneUnitKind::VillagerMiningGold => 6,
            SceneUnitKind::VillagerCarryingGold | SceneUnitKind::VillagerCarryingStone => 7,
            SceneUnitKind::VillagerMiningStone => 8,
            _ => usize::from(unit.moving),
        };
        (&art.village[group], 15)
    };
    let animated = unit.moving
        || matches!(
            unit.kind,
            SceneUnitKind::VillagerChopping
                | SceneUnitKind::VillagerForaging
                | SceneUnitKind::VillagerMiningGold
                | SceneUnitKind::VillagerMiningStone
        );
    (frames.len() >= stride * 5).then_some((frames, stride, animated))
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
    // Rear wings sit behind the central balcony, then the front supports.
    // The right roof already includes its front column: its native hotspot
    // needs only half the left roof's vertical attachment. The single front
    // left and rear right posts meet their eaves at +24/-24 source pixels.
    // Never rotate or mirror these sunlit parts; all share one ground origin.
    for part in [7, 3, 5, 1, 4, 0, 2, 6] {
        let mut native = art.town_center[part][0];
        native.anchor[1] -= [0.0, 48.0, 24.0, 0.0, 24.0, 0.0, -24.0, 0.0][part];
        let frame = world_frame(native);
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
