//! Actor timelines and native headings, shared by the rendered scene.
use super::*;

/// Whether `unit` has got under a fifth of its native stride ahead over the
/// last few ticks (about 0.6 s); shuffling back and forth gets nowhere.
pub(super) fn stalled(client: &Client, unit: &aoe_protocol::GameplayUnitState) -> bool {
    let hz = f64::from(client.config.tick_hz.max(1));
    let ticks = crate::unit_visuals::stall_window_ticks(hz);
    unit.hp > 0
        && client
            .presentation
            .progress_over(unit.id, ticks)
            .is_some_and(|travelled| {
                crate::unit_visuals::is_stalled(unit.kind, travelled, ticks / hz)
            })
}

/// The unit as drawn and its pose: a unit flagged as moving that has stood
/// still shows the rest pose matching what it would otherwise walk as.
pub(super) fn shown_pose(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
) -> (aoe_protocol::GameplayUnitState, crate::unit_visuals::Pose) {
    use crate::unit_visuals as visuals;
    if client
        .presentation
        .trebuchet_frame(unit, client.last_frame)
        .is_some()
    {
        return (*unit, visuals::Pose::TrebuchetTransition);
    }
    let animal = unit
        .hunt_target
        .and_then(|id| client.village.animals.iter().find(|a| a.id == id));
    let pose = if visuals::walks_as_villager(unit, animal) {
        visuals::Pose::VillagerWalk
    } else {
        visuals::lifecycle_pose(
            visuals::scene_pose(unit, animal, super::target_distance(client, unit)),
            unit.death_tick,
            client.village.tick,
            client.config.tick_hz,
        )
    };
    let at_rest = unit.moving && stalled(client, unit);
    let mut shown = *unit;
    shown.moving &= !at_rest;
    (
        shown,
        if at_rest {
            visuals::rest_pose(pose)
        } else {
            pose
        },
    )
}

pub(super) fn death_effect(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
    body: aoe_rendering::SceneUnit,
) -> Option<aoe_rendering::SceneUnit> {
    use crate::unit_visuals::Pose;
    use aoe_protocol::UnitKind;
    let since = unit.death_tick?;
    let pose = match unit.kind {
        UnitKind::Petard => Pose::PetardBlast,
        UnitKind::DemolitionRaft | UnitKind::DemolitionShip | UnitKind::HeavyDemolitionShip => {
            Pose::DemolitionBlast
        }
        _ => return None,
    };
    if client
        .art
        .actors
        .get(pose as usize)
        .is_none_or(Vec::is_empty)
    {
        return None;
    }
    let live = client
        .presentation
        .live_tick(client.last_frame)
        .unwrap_or(client.village.tick as f64);
    let elapsed =
        (live - since.0 as f64).max(0.0) * 1000.0 / f64::from(client.config.tick_hz.max(1));
    let source = aoe_assets::catalog::units::SPRITES[pose as usize];
    let phase = elapsed / source.frame_millis();
    if phase >= f64::from(source.stride) {
        return None;
    }
    Some(
        aoe_rendering::SceneUnit {
            // Keep the effect distinct from its hull and from projectile actors.
            id: aoe_core::EntityId(0x4000_0000 | (unit.id.0 & 0x3fff_ffff)),
            kind: aoe_rendering::SceneUnitKind::actor(pose as u16),
            visual: aoe_rendering::SceneUnitVisual::new(true),
            ..body
        }
        .with_actor_frame((phase as u16) << 4),
    )
}

pub(super) fn animation_frame(
    client: &Client,
    unit: &aoe_protocol::GameplayUnitState,
    pose: crate::unit_visuals::Pose,
    travelled: f64,
) -> u16 {
    if pose == crate::unit_visuals::Pose::TrebuchetTransition
        && let Some(phase) = client.presentation.trebuchet_frame(unit, client.last_frame)
    {
        let direction = (20 - u16::from(unit.facing % 8) * 2) % 16;
        return direction | (phase << 4);
    }
    // A walking unit's frame follows the distance it has really covered.
    let walking = unit.moving && unit.hp > 0 && unit.death_tick.is_none();
    let clock = if walking {
        crate::unit_visuals::walk_clock(unit.kind, travelled)
    } else {
        super::projectiles::pose_clock(client, unit, pose)
    };
    let mut frame = crate::unit_visuals::packed_frame(
        pose,
        unit.facing,
        clock,
        unit.death_tick,
        client
            .presentation
            .live_tick(client.last_frame)
            .unwrap_or(client.village.tick as f64),
        client.config.tick_hz,
    );
    if !matches!(
        pose,
        crate::unit_visuals::Pose::Arrow
            | crate::unit_visuals::Pose::Javelin
            | crate::unit_visuals::Pose::Bolt
    ) && let Some(heading) = client.presentation.native_heading(*unit)
    {
        frame = (frame & !15) | u16::from(heading);
    }
    frame
}
