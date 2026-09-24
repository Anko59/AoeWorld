use super::{
    SourceQualificationError, SourceQualificationProgress,
    metrics::{NavigationCachePeaks, ProcessRssSampler},
    route::{classify_route_failure, issue_leg},
};
use crate::PageResidency;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::MapPackage;
use aoe_simulation::{GameWorld, GameWorldError, StartSearchResult};

const HASH_CHECK_INTERVAL: u64 = 8_192;

pub(super) struct MovementRun {
    pub(super) start_tile: TileCoord,
    pub(super) route_waypoints: Vec<TileCoord>,
    pub(super) movement_ticks: u64,
    pub(super) route_moved_meters: f64,
    pub(super) replay_moved_meters: f64,
    pub(super) route_checkpoint_count: usize,
    pub(super) movement_replay_comparison_count: usize,
    pub(super) route_movement_leg_count: usize,
    pub(super) replay_movement_leg_count: usize,
    pub(super) peak_resident_pages: usize,
    pub(super) navigation_cache_peaks: NavigationCachePeaks,
    pub(super) replay_hash: [u8; 32],
}

pub(super) struct MovementInput<'a> {
    pub(super) package: &'a MapPackage,
    pub(super) generator: &'a aoe_map::MapChunkGenerator,
    pub(super) movement_provider: std::sync::Arc<PageResidency>,
    pub(super) replay_provider: std::sync::Arc<PageResidency>,
    pub(super) initial_peak_resident_pages: usize,
    pub(super) max_ticks: u64,
    pub(super) progress: &'a mut dyn FnMut(SourceQualificationProgress),
    pub(super) rss: &'a mut ProcessRssSampler,
}

pub(super) fn exercise_movement(
    input: MovementInput<'_>,
) -> Result<MovementRun, SourceQualificationError> {
    let MovementInput {
        package,
        generator,
        movement_provider,
        replay_provider,
        initial_peak_resident_pages,
        max_ticks,
        progress,
        rss,
    } = input;
    let mut world = GameWorld::from_page_provider(
        package.clone(),
        movement_provider.clone() as std::sync::Arc<dyn aoe_map::EnvironmentPageProvider>,
    )?;
    let mut replay = GameWorld::from_page_provider(
        package.clone(),
        replay_provider.clone() as std::sync::Arc<dyn aoe_map::EnvironmentPageProvider>,
    )?;
    let config = world.config();
    let start = match world.terrain().search_start_for_recipe(
        config,
        package.generation_recipe_version,
        64,
        || false,
    )? {
        StartSearchResult::Found(tile) => tile,
        StartSearchResult::Unavailable
        | StartSearchResult::LimitReached
        | StartSearchResult::Cancelled => return Err(SourceQualificationError::NoStart),
    };
    let unit = world.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let replay_unit = replay.spawn_unit(PlayerId(0), WorldPosition::from_tile_center(start)?)?;
    let axis = config.width_tiles;
    let center_y = (config.height_tiles - 1) / 2;
    let waypoints = [
        TileCoord::new(axis - 2, center_y),
        TileCoord::new(1, center_y),
    ];
    let mut navigation_cache_peaks = NavigationCachePeaks::default();
    let mut movement_replay_comparison_count = 0_usize;
    let mut route_movement_leg_count = 0_usize;
    let mut replay_movement_leg_count = 0_usize;
    issue_leg(
        &mut world,
        generator,
        package,
        &movement_provider,
        unit,
        waypoints[0],
    )?;
    route_movement_leg_count += 1;
    issue_leg(
        &mut replay,
        generator,
        package,
        &replay_provider,
        replay_unit,
        waypoints[0],
    )?;
    replay_movement_leg_count += 1;
    navigation_cache_peaks.observe(&world, &replay);

    let mut leg = 0_usize;
    let mut replay_leg = 0_usize;
    let mut last_position = world
        .unit(unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    let mut replay_last_position = replay
        .unit(replay_unit)
        .ok_or(GameWorldError::UnknownEntity)?
        .position;
    let mut moved_subunits = 0.0_f64;
    let mut replay_moved_subunits = 0.0_f64;
    let mut route_checkpoint_count = 0_usize;
    let mut movement_ticks = 0_u64;
    let mut peak_resident_pages = initial_peak_resident_pages
        .max(movement_provider.resident_pages())
        .max(replay_provider.resident_pages());
    for tick in 1..=max_ticks {
        world.advance();
        replay.advance();
        movement_ticks = tick;
        let state = world.unit(unit).ok_or(GameWorldError::UnknownEntity)?;
        let replay_state = replay
            .unit(replay_unit)
            .ok_or(GameWorldError::UnknownEntity)?;
        moved_subunits += displacement(last_position, state.position);
        replay_moved_subunits += displacement(replay_last_position, replay_state.position);
        last_position = state.position;
        replay_last_position = replay_state.position;
        peak_resident_pages = peak_resident_pages
            .max(movement_provider.resident_pages())
            .max(replay_provider.resident_pages());
        navigation_cache_peaks.observe(&world, &replay);
        if let Some(error) = world.movement_failure(unit) {
            return Err(classify_route_failure(
                error,
                waypoints[leg.min(waypoints.len() - 1)],
            ));
        }
        if let Some(error) = replay.movement_failure(replay_unit) {
            return Err(classify_route_failure(
                error,
                waypoints[replay_leg.min(waypoints.len() - 1)],
            ));
        }
        if tick % HASH_CHECK_INTERVAL == 0 {
            rss.observe();
            movement_replay_comparison_count += 1;
            let route_hash = world.canonical_hash();
            let replay_hash = replay.canonical_hash();
            if route_hash != replay_hash || state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            route_checkpoint_count += 1;
        }
        if world.movement_order(unit).is_none() {
            leg += 1;
            if leg < waypoints.len() {
                issue_leg(
                    &mut world,
                    generator,
                    package,
                    &movement_provider,
                    unit,
                    waypoints[leg],
                )?;
                route_movement_leg_count += 1;
            }
        }
        if replay.movement_order(replay_unit).is_none() {
            replay_leg += 1;
            if replay_leg < waypoints.len() {
                issue_leg(
                    &mut replay,
                    generator,
                    package,
                    &replay_provider,
                    replay_unit,
                    waypoints[replay_leg],
                )?;
                replay_movement_leg_count += 1;
            }
        }
        if leg == waypoints.len() && replay_leg == waypoints.len() {
            movement_replay_comparison_count += 1;
            let route_hash = world.canonical_hash();
            let replay_hash = replay.canonical_hash();
            if route_hash != replay_hash || state != replay_state {
                return Err(SourceQualificationError::ReplayDiverged(tick));
            }
            break;
        }
        if tick % 50_000 == 0 {
            progress(SourceQualificationProgress {
                tick,
                leg,
                moved_meters: moved_subunits / 1_024.0 * 2.0,
            });
        }
        if tick == max_ticks {
            return Err(SourceQualificationError::TickLimit);
        }
    }
    movement_replay_comparison_count += 1;
    let route_hash = world.canonical_hash();
    let replay_hash = replay.canonical_hash();
    if route_hash != replay_hash {
        return Err(SourceQualificationError::ReplayDiverged(movement_ticks));
    }
    let route_moved_meters = moved_subunits / 1_024.0 * 2.0;
    let replay_moved_meters = replay_moved_subunits / 1_024.0 * 2.0;
    if route_moved_meters < 100_000.0 || replay_moved_meters != route_moved_meters {
        return Err(SourceQualificationError::InsufficientDistance(
            route_moved_meters,
        ));
    }
    navigation_cache_peaks.observe(&world, &replay);
    peak_resident_pages = peak_resident_pages
        .max(movement_provider.resident_pages())
        .max(replay_provider.resident_pages());
    rss.observe();
    Ok(MovementRun {
        start_tile: start,
        route_waypoints: waypoints.to_vec(),
        movement_ticks,
        route_moved_meters,
        replay_moved_meters,
        route_checkpoint_count,
        movement_replay_comparison_count,
        route_movement_leg_count,
        replay_movement_leg_count,
        peak_resident_pages,
        navigation_cache_peaks,
        replay_hash: route_hash,
    })
}

fn displacement(from: WorldPosition, to: WorldPosition) -> f64 {
    let dx = f64::from(to.x) - f64::from(from.x);
    let dy = f64::from(to.y) - f64::from(from.y);
    dx.hypot(dy)
}
