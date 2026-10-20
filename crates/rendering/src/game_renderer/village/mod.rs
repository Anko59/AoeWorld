//! Reviewed native villager rows and the multipart Dark Age Town Center.
use super::*;
use crate::SceneUnitKind;

pub(super) fn frames(art: &GameArt, unit: SceneUnit) -> Option<(&[GameFrame], usize, bool)> {
    if let Some(index) = unit.kind.actor_slot() {
        let frames = art.actors.get(index)?.as_slice();
        let stride = frames.len() / 5;
        return (stride > 0 && frames.len() == stride * 5).then_some((frames, stride, false));
    }
    let scout = unit.kind == SceneUnitKind::Scout;
    let (frames, stride): (&[GameFrame], usize) = if scout {
        (
            if unit.moving {
                &art.walking
            } else {
                &art.standing
            },
            if unit.moving { 10 } else { 1 },
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
        (
            if unit.kind == SceneUnitKind::VillagerBuilding {
                &art.buildings[22]
            } else {
                &art.village[group]
            },
            if group == 0 && unit.kind != SceneUnitKind::VillagerBuilding {
                1
            } else {
                15
            },
        )
    };
    let animated = unit.moving
        || matches!(
            unit.kind,
            SceneUnitKind::VillagerBuilding
                | SceneUnitKind::VillagerChopping
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
    let side = unit.kind.building_side();
    let building = side.is_some();
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
            [
                side.unwrap_or(1.0) as f32 * 30.0,
                side.unwrap_or(1.0) as f32 * 15.0,
            ]
        } else {
            [14.0, 32.0]
        },
        anchor: if building {
            [
                side.unwrap_or(1.0) as f32 * 15.0,
                side.unwrap_or(1.0) as f32 * 7.5,
            ]
        } else {
            [7.0, 30.0]
        },
    };
    if let Some(mut sprite) = scene_sprite(frame, unit.position, unit.elevation_meters, camera) {
        sprite.color = if building {
            [0.72, 0.58, 0.35, 1.0]
        } else if matches!(
            unit.kind,
            SceneUnitKind::NeutralSheepStand
                | SceneUnitKind::NeutralSheepWalk
                | SceneUnitKind::NeutralSheepDie
                | SceneUnitKind::NeutralSheepDecay
                | SceneUnitKind::DeerStand
                | SceneUnitKind::DeerWalk
                | SceneUnitKind::DeerDie
                | SceneUnitKind::DeerDecay
                | SceneUnitKind::BoarStand
                | SceneUnitKind::BoarWalk
                | SceneUnitKind::BoarAttack
                | SceneUnitKind::BoarDie
                | SceneUnitKind::BoarDecay
                | SceneUnitKind::WolfStand
                | SceneUnitKind::WolfWalk
                | SceneUnitKind::WolfRun
                | SceneUnitKind::WolfAttack
                | SceneUnitKind::WolfDie
                | SceneUnitKind::WolfDecay
        ) {
            // Missing native art must not make neutral wildlife look player-owned.
            [0.75, 0.75, 0.75, 1.0]
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

/// Native anchors and player/shadow planes are shared by all backends.
pub(super) fn building(
    art: &GameArt,
    unit: SceneUnit,
    camera: SceneCamera,
    animation: usize,
    emit: &mut impl FnMut((Sprite, GameFrame, f64, u64)),
) {
    let Some(art_slot) = unit.kind.building_slot() else {
        return;
    };
    if art_slot == 5 {
        return;
    } // Farm is ground.
    let frames = art
        .buildings
        .get(usize::from(art_slot))
        .map_or(&[][..], |frames| frames.as_slice());
    if frames.is_empty() || (art_slot == 4 && frames.len() != 44) {
        marker(unit, camera, emit);
        return;
    }
    let index = if (9..=12).contains(&art_slot) {
        usize::from(unit.facing).min(frames.len() - 1)
    } else if art_slot == 7 {
        usize::from(unit.facing) % frames.len()
    } else {
        unit.id.0 as usize % frames.len()
    };
    let (indices, count) = if art_slot == 4 {
        ([0, 1 + animation % 43], 2)
    } else {
        ([index, 0], 1)
    };
    for index in indices.into_iter().take(count) {
        let frame = world_frame(frames[index]);
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
