//! Fixed local actions and snapshots; no browser, process, report, or verdict inputs.
use super::{Action, MoveOutcome, Order, Snapshot, Trace, Unit};
use aoe_core::{EntityId, PlayerId, Seed, TileRect, WorldConfig, WorldPosition};
use aoe_simulation::{GameWorld, GameWorldError};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum Script {
    Normal,
    OmittedOrder,
    #[cfg(test)]
    WrongTarget,
    #[cfg(test)]
    UnderTicks,
    #[cfg(test)]
    UnknownEntity,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum SetupFailure {
    Configuration,
    World,
    Spawn,
    MissingUnit,
}

#[derive(Debug)]
pub(super) struct Failure {
    pub(super) reason: SetupFailure,
    pub(super) executed_actions: u64,
}
impl From<SetupFailure> for Failure {
    fn from(reason: SetupFailure) -> Self {
        Self {
            reason,
            executed_actions: 0,
        }
    }
}
fn captured(world: &GameWorld, id: EntityId, executed_actions: u64) -> Result<Snapshot, Failure> {
    snapshot(world, id).map_err(|reason| Failure {
        reason,
        executed_actions,
    })
}

fn outcome(result: Result<bool, GameWorldError>) -> MoveOutcome {
    match result {
        Ok(true) => MoveOutcome::Accepted,
        Ok(false) => MoveOutcome::NoMovement,
        Err(GameWorldError::UnknownEntity) => MoveOutcome::UnknownEntity,
        Err(_) => MoveOutcome::OtherRejection,
    }
}

fn snapshot(world: &GameWorld, id: EntityId) -> Result<Snapshot, SetupFailure> {
    let unit = world.unit(id).ok_or(SetupFailure::MissingUnit)?;
    let ids = |rect| world.query(rect).0.iter().map(|unit| unit.id.0).collect();
    Ok(Snapshot {
        tick: world.tick().0,
        unit: Unit {
            id: unit.id.0,
            player: unit.player.0,
            position: unit.position,
            previous_position: unit.previous_position,
            moving: unit.moving,
            planning: unit.planning,
            facing: unit.facing as u8,
        },
        order: world.movement_order(id).map(|order| Order {
            origin: order.origin,
            destination: order.destination,
            waypoint: order.waypoint,
            target_tile: order.target_tile,
            segment_length: order.segment_length,
            travelled: order.travelled,
            speed_carry: order.speed_carry,
        }),
        active_movers: world.active_mover_count() as u64,
        target_units: ids(TileRect::from_xywh(33, 31, 1, 1)),
        origin_units: ids(TileRect::from_xywh(31, 31, 1, 1)),
    })
}

pub(super) fn perform(script: Script) -> Result<Trace, Failure> {
    let mut config =
        WorldConfig::new(256, 256, Seed(7)).map_err(|_| SetupFailure::Configuration)?;
    config.move_speed_subunits_per_tick = 128;
    config.move_speed_subunits_per_tick_denominator = 1;
    let mut world = GameWorld::new(config).map_err(|_| SetupFailure::World)?;
    // These fixed coordinates are the independently specified tile-center oracle.
    let origin = WorldPosition::new(32_256, 32_256);
    let target = WorldPosition::new(34_304, 32_256);
    let id = world
        .spawn_unit(PlayerId(0), origin)
        .map_err(|_| SetupFailure::Spawn)?;
    let initial = captured(&world, id, 0)?;
    let unknown_result = outcome(world.issue_move(EntityId(999), target));
    let mut actions = vec![Action::MoveOrder {
        entity_id: 999,
        destination: target,
        outcome: unknown_result,
    }];
    let after_unknown = captured(&world, id, actions.len() as u64)?;
    let mut move_result = None;
    let (destination, ordered_id, ticks) = match script {
        Script::Normal | Script::OmittedOrder => (target, id, 16),
        #[cfg(test)]
        Script::WrongTarget => (WorldPosition::new(33_280, 32_256), id, 16),
        #[cfg(test)]
        Script::UnknownEntity => (target, EntityId(999), 16),
        #[cfg(test)]
        Script::UnderTicks => (target, id, 15),
    };
    if script != Script::OmittedOrder {
        let observed = outcome(world.issue_move(ordered_id, destination));
        move_result = Some(observed);
        actions.push(Action::MoveOrder {
            entity_id: ordered_id.0,
            destination,
            outcome: observed,
        });
    }
    let after_order = captured(&world, id, actions.len() as u64)?;
    let mut steps = Vec::with_capacity(16);
    for _ in 0..ticks {
        world.advance();
        actions.push(Action::Advance {
            tick: world.tick().0,
        });
        steps.push(captured(&world, id, actions.len() as u64)?);
    }
    let executed_actions = actions.len() as u64;
    Ok(Trace {
        script,
        actions,
        initial,
        unknown_result,
        after_unknown,
        move_result,
        after_order,
        steps,
        executed_actions,
    })
}
