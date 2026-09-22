use super::{SourceQualificationError, diagnostics::start_diagnostic};
use crate::PageResidency;
use aoe_core::{TileCoord, WorldPosition};
use aoe_map::{
    MAX_ROUTE_PLANNER_NODES, MAX_ROUTE_PLANNER_WORK, MapChunkGenerator, MapPackage,
    RoutePlannerPoll,
};
use aoe_simulation::{GameWorld, GameWorldError};

pub(super) fn issue_leg(
    world: &mut GameWorld,
    generator: &MapChunkGenerator,
    package: &MapPackage,
    provider: &PageResidency,
    id: aoe_core::EntityId,
    destination: TileCoord,
) -> Result<(), SourceQualificationError> {
    let position = WorldPosition::from_tile_center(destination)?;
    let destination_tile = position.tile_floor();
    if !world
        .terrain()
        .passable_with_cancel(destination_tile, world.config(), &|| false)?
    {
        return Err(SourceQualificationError::ImpassableWaypoint {
            x: destination.x,
            y: destination.y,
        });
    }
    let origin = world
        .unit(id)
        .ok_or(GameWorldError::UnknownEntity)?
        .position
        .tile_floor();
    if let Err(error) = world.issue_move(id, position) {
        if matches!(&error, GameWorldError::Unreachable) {
            let diagnostic = unreachable_route_diagnostic(
                world,
                generator,
                package,
                provider,
                origin,
                destination_tile,
                destination,
            )?;
            return Err(SourceQualificationError::UnreachableWaypoint {
                x: destination.x,
                y: destination.y,
                diagnostic,
            });
        }
        return Err(classify_route_failure(error, destination));
    }
    Ok(())
}

fn unreachable_route_diagnostic(
    world: &GameWorld,
    generator: &MapChunkGenerator,
    package: &MapPackage,
    provider: &PageResidency,
    origin: TileCoord,
    destination_tile: TileCoord,
    destination: TileCoord,
) -> Result<String, SourceQualificationError> {
    let endpoint = endpoint_diagnostic(generator, destination)?;
    let route_probe = route_probe(world, origin, destination_tile)?;
    let nearby = nearby_waypoint_probes(world, origin, destination)?;
    let starts = start_diagnostic(
        world.terrain(),
        generator,
        package,
        provider,
        world.config(),
        64,
        Some(origin),
    )?;
    Ok(format!(
        "start_tile=({},{}), start_connected_component={}, A*_outcome={}, A*_work={}/{MAX_ROUTE_PLANNER_WORK}, A*_peak_entries={}/{MAX_ROUTE_PLANNER_NODES}, {endpoint}, nearby_fixed_waypoint_probes={nearby}, {starts}",
        origin.x,
        origin.y,
        route_probe.connected,
        route_probe.outcome,
        route_probe.work,
        route_probe.peak_entries,
    ))
}

struct RouteProbe {
    connected: &'static str,
    outcome: &'static str,
    work: u32,
    peak_entries: usize,
}

fn route_probe(
    world: &GameWorld,
    origin: TileCoord,
    destination: TileCoord,
) -> Result<RouteProbe, SourceQualificationError> {
    let Some(mut planner) =
        world
            .terrain()
            .route_planner(origin, destination, MAX_ROUTE_PLANNER_WORK)
    else {
        return Err(GameWorldError::InvalidTerrain.into());
    };
    let result = world
        .terrain()
        .poll_route_planner(&mut planner, MAX_ROUTE_PLANNER_WORK, &|| false)
        .ok_or(GameWorldError::InvalidTerrain)?;
    let (connected, outcome) = match result {
        RoutePlannerPoll::Path(_) => ("true", "path_segment"),
        RoutePlannerPoll::Complete => ("true", "complete"),
        RoutePlannerPoll::Unreachable => ("false", "unreachable"),
        RoutePlannerPoll::InvalidDestination => ("false", "invalid_destination"),
        RoutePlannerPoll::SearchLimit => ("unknown", "search_limit"),
        RoutePlannerPoll::Pending => ("unknown", "pending"),
        RoutePlannerPoll::Environment(error) => {
            return Err(SourceQualificationError::Page(error));
        }
    };
    Ok(RouteProbe {
        connected,
        outcome,
        work: planner.work(),
        peak_entries: planner.peak_retained_entries(),
    })
}

fn nearby_waypoint_probes(
    world: &GameWorld,
    origin: TileCoord,
    failed_east_waypoint: TileCoord,
) -> Result<String, SourceQualificationError> {
    const EAST_EDGE_Y_OFFSETS: [i32; 8] = [-128, -32, -8, -1, 1, 8, 32, 128];
    const EAST_EDGE_X_OFFSETS: [i32; 4] = [-128, -32, -8, -1];
    let config = world.config();
    let west_endpoint = TileCoord::new(1, (config.height_tiles - 1) / 2);
    let mut candidates = EAST_EDGE_Y_OFFSETS
        .iter()
        .map(|offset| {
            (
                format!("east_y{offset:+}"),
                TileCoord::new(failed_east_waypoint.x, failed_east_waypoint.y + offset),
            )
        })
        .collect::<Vec<_>>();
    candidates.extend(EAST_EDGE_X_OFFSETS.iter().map(|offset| {
        (
            format!("east_x{offset:+}"),
            TileCoord::new(failed_east_waypoint.x + offset, failed_east_waypoint.y),
        )
    }));
    candidates.push(("west_endpoint".to_owned(), west_endpoint));
    candidates.extend(EAST_EDGE_Y_OFFSETS.iter().map(|offset| {
        (
            format!("west_y{offset:+}"),
            TileCoord::new(west_endpoint.x, west_endpoint.y + offset),
        )
    }));
    let mut probes = Vec::with_capacity(candidates.len());
    for (label, destination) in candidates {
        if destination.y < 0 || destination.y >= config.height_tiles {
            probes.push(format!("{label}=outside"));
            continue;
        }
        let passable = world
            .terrain()
            .passable_with_cancel(destination, config, &|| false)?;
        if !passable {
            probes.push(format!("{label}=impassable"));
            continue;
        }
        let result = route_probe(world, origin, destination)?;
        probes.push(format!(
            "{label}={}/{}:{}/{}",
            result.connected, result.outcome, result.work, result.peak_entries
        ));
    }
    Ok(format!("[{}]", probes.join(";")))
}

fn endpoint_diagnostic(
    generator: &MapChunkGenerator,
    destination: TileCoord,
) -> Result<String, SourceQualificationError> {
    let tile = generator
        .tile_at_with_cancel(destination, &|| false)?
        .ok_or(aoe_map::EnvironmentPageError::Invalid)?;
    let resource = generator
        .object_at_with_cancel(destination, &|| false)?
        .map(|node| (node.kind, node.object, node.id));
    let mut max_adjacent_rise_cm = 0_i32;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neighbor = TileCoord::new(destination.x + dx, destination.y + dy);
        let Some(sample) = generator.tile_at_with_cancel(neighbor, &|| false)? else {
            continue;
        };
        max_adjacent_rise_cm = max_adjacent_rise_cm
            .max((tile.geographic_height_centimeters - sample.geographic_height_centimeters).abs());
    }
    let edge_grade_percent = f64::from(max_adjacent_rise_cm) / 2.0;
    Ok(format!(
        "endpoint_material={:?}, endpoint_water={:?}, endpoint_resource={resource:?}, endpoint_passable={}, endpoint_surface={:?}, endpoint_height_cm={}, endpoint_max_adjacent_rise_cm={max_adjacent_rise_cm}, endpoint_max_2m_edge_grade_percent={edge_grade_percent:.2}",
        tile.material, tile.water, tile.passable, tile.surface, tile.geographic_height_centimeters,
    ))
}

pub(super) fn classify_route_failure(
    error: GameWorldError,
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
            diagnostic: "movement planner returned unreachable after route qualification".into(),
        },
        error => SourceQualificationError::Movement(error),
    }
}
