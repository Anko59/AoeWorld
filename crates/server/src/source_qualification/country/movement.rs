//! Fixed 64m and original-endpoint 256m native orders; direct probes stay unchanged.
use super::{
    SourceQualificationError,
    diagnostics::{DIRECTIONS, LOCAL_DISTANCE},
};
use crate::PageResidency;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::{EnvironmentPageProvider, MapPackage};
use aoe_simulation::{GameWorld, GameWorldError};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub(super) enum OrderCase {
    Nearby32,
    Original128,
}

pub(super) const ORDER_CASES: [OrderCase; 2] = [OrderCase::Nearby32, OrderCase::Original128];

impl OrderCase {
    pub(super) const fn distance_tiles(self) -> i32 {
        match self {
            Self::Nearby32 => 32,
            Self::Original128 => LOCAL_DISTANCE,
        }
    }

    pub(super) const fn maximum_ticks(self) -> usize {
        match self {
            Self::Nearby32 => 2048,
            Self::Original128 => 8192,
        }
    }

    pub(super) const fn policy(self) -> &'static str {
        match self {
            Self::Nearby32 => "native-country-four-fixed-32-tile-orders-2048-ticks-v1",
            Self::Original128 => "native-country-original-four-fixed-128-tile-orders-8192-ticks-v1",
        }
    }

    pub(super) fn destination(self, origin: TileCoord, direction: (i32, i32)) -> TileCoord {
        TileCoord::new(
            origin.x + direction.0 * self.distance_tiles(),
            origin.y + direction.1 * self.distance_tiles(),
        )
    }
}

#[derive(Debug, Serialize)]
pub struct NativeLocalOrder {
    pub policy: &'static str,
    pub origin: [i32; 2],
    pub destination: [i32; 2],
    pub requested_distance_tiles: u32,
    pub requested_game_meters: u32,
    pub destination_geometrically_valid: bool,
    pub destination_passable: bool,
    pub ticks_advanced: usize,
    pub maximum_ticks: usize,
    pub outcome: &'static str,
    pub failure: Option<String>,
    pub final_position_subunits: [i32; 2],
    pub hardware_qualified: bool,
}

pub(super) fn observe(
    package: &MapPackage,
    provider: &Arc<PageResidency>,
    origin: TileCoord,
) -> Result<Vec<NativeLocalOrder>, SourceQualificationError> {
    let mut results = Vec::with_capacity(ORDER_CASES.len() * DIRECTIONS.len());
    // No adaptive target selection: preserve every case and cardinal direction.
    // A fresh world per order prevents preceding movement from affecting its state.
    for case in ORDER_CASES {
        for direction in DIRECTIONS {
            let world = GameWorld::from_page_provider(
                package.clone(),
                provider.clone() as Arc<dyn EnvironmentPageProvider>,
            )?;
            let destination = case.destination(origin, direction);
            results.push(advance_order(world, origin, destination, case)?);
        }
    }
    Ok(results)
}

pub(super) fn advance_order(
    mut world: GameWorld,
    origin: TileCoord,
    destination: TileCoord,
    case: OrderCase,
) -> Result<NativeLocalOrder, SourceQualificationError> {
    let position = WorldPosition::from_tile_center(origin)?;
    let target = WorldPosition::from_tile_center(destination)?;
    let id = world.spawn_unit(PlayerId(0), position)?;
    let mut report = NativeLocalOrder {
        policy: case.policy(),
        origin: [origin.x, origin.y],
        destination: [destination.x, destination.y],
        requested_distance_tiles: case.distance_tiles() as u32,
        requested_game_meters: case.distance_tiles() as u32 * 2,
        destination_geometrically_valid: world.config().valid_ground_position(target),
        destination_passable: world.terrain().passable_with_cancel(
            destination,
            world.config(),
            &|| false,
        )?,
        ticks_advanced: 0,
        maximum_ticks: case.maximum_ticks(),
        outcome: "tick_limit",
        failure: None,
        final_position_subunits: [position.x, position.y],
        hardware_qualified: false,
    };
    if let Err(error) = world.issue_move(id, target) {
        if let GameWorldError::Environment(_) = error {
            return Err(error.into());
        }
        report.outcome = "rejected";
        report.failure = Some(error.to_string());
        return Ok(report);
    }
    for tick in 1..=case.maximum_ticks() {
        world.advance();
        report.ticks_advanced = tick;
        let unit = world.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        report.final_position_subunits = [unit.position.x, unit.position.y];
        if unit.position == target && !unit.moving && !unit.planning {
            report.outcome = "arrived";
            break;
        }
        if let Some(error) = world.movement_failure(id) {
            if let GameWorldError::Environment(_) = error {
                return Err(error.into());
            }
            report.outcome = "movement_failed";
            report.failure = Some(error.to_string());
            break;
        }
    }
    Ok(report)
}
