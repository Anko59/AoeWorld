use super::SourceQualificationError;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord, WorldConfig, WorldPosition};
use aoe_map::{EnvironmentPageError, MapChunkGenerator};
use aoe_simulation::{GameWorldError, Terrain};

pub(super) const ROUTE_CONTRACT: &str = "fixed-cardinal-repeat-within-ordinary-component-v1";
pub(super) const REQUIRED_TRAVEL_METERS: f64 = 100_000.0;
pub(super) const REQUIRED_REPETITIONS: u64 = 25_000;
const METERS_PER_TILE: f64 = 2.0;
const PREFERRED_OFFSET: (i32, i32) = (2, 0);
const CARDINAL_OFFSETS: [(i32, i32); 4] = [(2, 0), (0, 2), (-2, 0), (0, -2)];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FixedRoute {
    pub(super) start: TileCoord,
    pub(super) alternate: TileCoord,
    pub(super) offset: (i32, i32),
    pub(super) leg_length_tiles: u32,
    pub(super) leg_length_meters: f64,
    pub(super) repetitions: u64,
}

impl FixedRoute {
    pub(super) fn waypoints(self) -> [TileCoord; 2] {
        [self.start, self.alternate]
    }

    pub(super) fn spatial_extent_tiles(self) -> [i32; 2] {
        [
            self.start.x.abs_diff(self.alternate.x) as i32,
            self.start.y.abs_diff(self.alternate.y) as i32,
        ]
    }

    pub(super) fn destination_for_leg(self, leg_index: u64) -> TileCoord {
        if leg_index.is_multiple_of(2) {
            self.alternate
        } else {
            self.start
        }
    }

    pub(super) fn accumulated_distance_meters(self, legs: u64) -> f64 {
        f64::from(legs as u32) * self.leg_length_meters
    }

    pub(super) fn movement_waypoints(self) -> Result<Vec<WorldPosition>, SourceQualificationError> {
        let waypoints = (0..self.repetitions)
            .map(|leg| WorldPosition::from_tile_center(self.destination_for_leg(leg)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(waypoints)
    }

    pub(super) fn expected_ticks(
        self,
        config: WorldConfig,
    ) -> Result<u64, SourceQualificationError> {
        let leg_subunits =
            u128::from(self.leg_length_tiles) * u128::from(FIXED_SUBUNITS_PER_TILE.unsigned_abs());
        let distance_subunits = leg_subunits * u128::from(self.repetitions);
        let speed = u128::try_from(config.move_speed_subunits_per_tick).map_err(|_| {
            SourceQualificationError::FixedRouteTickBound {
                required: u64::MAX,
                maximum: 0,
            }
        })?;
        if speed == 0 || config.move_speed_subunits_per_tick_denominator == 0 {
            return Err(SourceQualificationError::FixedRouteTickBound {
                required: u64::MAX,
                maximum: 0,
            });
        }
        let numerator =
            distance_subunits * u128::from(config.move_speed_subunits_per_tick_denominator);
        let expected_ticks = numerator.div_ceil(speed);
        u64::try_from(expected_ticks)
            .ok()
            .ok_or(SourceQualificationError::FixedRouteTickBound {
                required: u64::MAX,
                maximum: 0,
            })
    }

    pub(super) fn ensure_tick_bound(
        self,
        config: WorldConfig,
        max_ticks: u64,
    ) -> Result<(), SourceQualificationError> {
        let required = self.expected_ticks(config)?;
        if required > max_ticks {
            return Err(SourceQualificationError::FixedRouteTickBound {
                required,
                maximum: max_ticks,
            });
        }
        Ok(())
    }
}

pub(super) fn plan_fixed_repeated_route(
    terrain: &Terrain,
    config: WorldConfig,
    start: TileCoord,
) -> Result<FixedRoute, SourceQualificationError> {
    plan_fixed_repeated_route_with(&TerrainFixedLegQuery { terrain, config }, start)
}

trait FixedLegQuery {
    fn passable(&self, tile: TileCoord) -> Result<bool, EnvironmentPageError>;

    fn crossable(&self, from: TileCoord, to: TileCoord) -> Result<bool, EnvironmentPageError>;
}

struct TerrainFixedLegQuery<'a> {
    terrain: &'a Terrain,
    config: WorldConfig,
}

impl FixedLegQuery for TerrainFixedLegQuery<'_> {
    fn passable(&self, tile: TileCoord) -> Result<bool, EnvironmentPageError> {
        self.terrain
            .passable_with_cancel(tile, self.config, &|| false)
    }

    fn crossable(&self, from: TileCoord, to: TileCoord) -> Result<bool, EnvironmentPageError> {
        self.terrain
            .crossable_with_cancel(from, to, self.config, &|| false)
    }
}

fn plan_fixed_repeated_route_with<Q: FixedLegQuery>(
    query: &Q,
    start: TileCoord,
) -> Result<FixedRoute, SourceQualificationError> {
    let mut attempts = Vec::<String>::new();
    for offset in CARDINAL_OFFSETS {
        let alternate = TileCoord::new(
            start.x.saturating_add(offset.0),
            start.y.saturating_add(offset.1),
        );
        if offset == PREFERRED_OFFSET {
            attempts.push(format!("preferred={offset:?}"));
        }
        let crossable = query.passable(start)?
            && query.passable(alternate)?
            && leg_is_crossable(query, start, alternate)?;
        if crossable {
            let leg_length_tiles = offset.0.unsigned_abs() + offset.1.unsigned_abs();
            let leg_length_meters = f64::from(leg_length_tiles) * METERS_PER_TILE;
            let repetitions = (REQUIRED_TRAVEL_METERS / leg_length_meters).ceil() as u64;
            return Ok(FixedRoute {
                start,
                alternate,
                offset,
                leg_length_tiles,
                leg_length_meters,
                repetitions,
            });
        }
        attempts.push(format!("offset={offset:?},crossable=false"));
    }
    Err(SourceQualificationError::FixedRouteLegUnavailable {
        start: [start.x, start.y],
        diagnostic: attempts.join(";"),
    })
}

fn leg_is_crossable<Q: FixedLegQuery>(
    query: &Q,
    start: TileCoord,
    alternate: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    let step_x = (alternate.x - start.x).signum();
    let step_y = (alternate.y - start.y).signum();
    let mut from = start;
    while from != alternate {
        let to = TileCoord::new(from.x + step_x, from.y + step_y);
        if !query.passable(to)? || !query.crossable(from, to)? || !query.crossable(to, from)? {
            return Ok(false);
        }
        from = to;
    }
    Ok(true)
}

pub(super) fn contextual_route_failure(
    error: GameWorldError,
    generator: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
) -> SourceQualificationError {
    match error {
        GameWorldError::InvalidPosition => SourceQualificationError::ImpassableWaypoint {
            x: destination.x,
            y: destination.y,
        },
        GameWorldError::Unreachable => SourceQualificationError::UnreachableWaypoint {
            x: destination.x,
            y: destination.y,
            diagnostic: unreachable_route_diagnostic(generator, origin, destination),
        },
        error => SourceQualificationError::Movement(error),
    }
}

fn unreachable_route_diagnostic(
    generator: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
) -> String {
    let destination_detail = destination_diagnostic(generator, destination)
        .unwrap_or_else(|error| format!("destination_read_error={error}"));
    format!(
        "contract={ROUTE_CONTRACT},origin=({},{}),failed_destination=({},{}),{destination_detail}",
        origin.x, origin.y, destination.x, destination.y,
    )
}

fn destination_diagnostic(
    generator: &MapChunkGenerator,
    destination: TileCoord,
) -> Result<String, SourceQualificationError> {
    let tile = generator
        .tile_at_with_cancel(destination, &|| false)?
        .ok_or(EnvironmentPageError::Invalid)?;
    let resource = generator
        .object_at_with_cancel(destination, &|| false)?
        .map(|node| (node.kind, node.object, node.id));
    Ok(format!(
        "destination_material={:?},destination_water={:?},destination_resource={resource:?},destination_passable={},destination_surface={:?},destination_height_cm={}",
        tile.material, tile.water, tile.passable, tile.surface, tile.geographic_height_centimeters,
    ))
}

#[cfg(test)]
mod tests;
