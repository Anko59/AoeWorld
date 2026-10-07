//! Additional fixed 64m native orders; never replace the four original 256m probes.
use super::{SourceQualificationError, diagnostics::DIRECTIONS};
use crate::PageResidency;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::{EnvironmentPageProvider, MapPackage};
use aoe_simulation::{GameWorld, GameWorldError};
use serde::Serialize;
use std::sync::Arc;

const DISTANCE_TILES: i32 = 32;
const MAX_TICKS: usize = 2048;

#[derive(Debug, Serialize)]
pub struct NativeLocalOrder {
    pub policy: &'static str,
    pub origin: [i32; 2],
    pub destination: [i32; 2],
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
    let mut results = Vec::with_capacity(4);
    for (dx, dy) in DIRECTIONS {
        let world = GameWorld::from_page_provider(
            package.clone(),
            provider.clone() as Arc<dyn EnvironmentPageProvider>,
        )?;
        let destination = TileCoord::new(
            origin.x + dx * DISTANCE_TILES,
            origin.y + dy * DISTANCE_TILES,
        );
        results.push(advance_order(world, origin, destination)?);
    }
    Ok(results)
}

pub(super) fn advance_order(
    mut world: GameWorld,
    origin: TileCoord,
    destination: TileCoord,
) -> Result<NativeLocalOrder, SourceQualificationError> {
    let position = WorldPosition::from_tile_center(origin)?;
    let target = WorldPosition::from_tile_center(destination)?;
    let id = world.spawn_unit(PlayerId(0), position)?;
    let mut report = NativeLocalOrder {
        policy: "native-country-four-fixed-32-tile-orders-2048-ticks-v1",
        origin: [origin.x, origin.y],
        destination: [destination.x, destination.y],
        requested_game_meters: 64,
        destination_geometrically_valid: world.config().valid_ground_position(target),
        destination_passable: world.terrain().passable_with_cancel(
            destination,
            world.config(),
            &|| false,
        )?,
        ticks_advanced: 0,
        maximum_ticks: MAX_TICKS,
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
    for tick in 1..=MAX_TICKS {
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
