//! Route-scheduler diagnostics for the real-match probe (`scale-match
//! --march-probe`), admin secret only. `GET /admin/match/routes` reports the
//! scheduler's occupancy and its work attribution; `POST
//! /admin/match/route_probe` plans the route a formation order of the given
//! units would take, outside the game's budgets, and its ideal time.
use super::authorize;
use crate::AppState;
use aoe_core::{EntityId, Formation, PlayerId, TileCoord};
use aoe_map::{MAX_ROUTE_PLANNER_WORK, RoutePlanner, RoutePlannerPoll};
use aoe_simulation::{
    ROUTE_CATEGORIES, ROUTE_DISTANCES, ROUTE_FAILURES, ROUTE_REACH, ROUTE_REQUESTERS,
    RouteAttribution, RouteQueueBreakdown, RouteSchedulerStats,
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Work per read-locked poll: the tick writer gets the lock between polls.
const PROBE_POLL_WORK: u32 = 32_768;
const PROBE_DEADLINE: Duration = Duration::from_secs(90);

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/match/routes", get(scheduler))
        .route("/admin/match/route_probe", post(route_probe))
}

type Error = (StatusCode, String);

/// The attribution as JSON (also the scheduler log line's `attribution`).
pub(crate) fn attribution_json(a: &RouteAttribution) -> Value {
    let mut classes = serde_json::Map::new();
    for (c, row) in a.classes.iter().enumerate() {
        for (d, cell) in row.iter().enumerate() {
            if cell.orders + cell.found + cell.failed + cell.expired == 0
                && cell.fair_work + cell.issue_work + cell.lookahead_work == 0
            {
                continue;
            }
            classes.insert(
                format!("{}/{}", ROUTE_CATEGORIES[c], ROUTE_DISTANCES[d]),
                json!({
                    "orders": cell.orders, "fast_work": cell.issue_work,
                    "fair_work": cell.fair_work, "lookahead_work": cell.lookahead_work,
                    "found": cell.found, "failed": cell.failed,
                    "failed_1_4ki": cell.failed_1_4ki, "failed_work": cell.failed_work,
                    "expired": cell.expired,
                }),
            );
        }
    }
    let callers: serde_json::Map<String, Value> = a
        .callers
        .iter()
        .map(|(at, c)| {
            let reasons: serde_json::Map<String, Value> = ROUTE_FAILURES
                .iter()
                .zip(c.reasons_1_4ki)
                .filter(|(_, n)| *n > 0)
                .map(|(name, n)| ((*name).to_owned(), json!(n)))
                .collect();
            let categories: serde_json::Map<String, Value> = ROUTE_CATEGORIES
                .iter()
                .zip(c.categories_1_4ki)
                .filter(|(_, n)| *n > 0)
                .map(|(name, n)| ((*name).to_owned(), json!(n)))
                .collect();
            (
                at.clone(),
                json!({
                    "orders": c.orders, "failed": c.failed, "failed_work": c.failed_work,
                    "failed_1_4ki": c.failed_1_4ki, "reasons_1_4ki": reasons,
                    "categories_1_4ki": categories, "cached_failures": c.cached_failures,
                }),
            )
        })
        .collect();
    json!({
        "classes": classes,
        "callers": callers,
        "group_orders": a.group_orders,
        "group_orders_expired": a.group_orders_expired,
        "group_members_expired": a.group_members_expired,
        "single_expired": a.single_expired,
        "group_orders_failed": a.group_orders_failed,
        "group_ends": {
            "wait_expired": a.group_orders_expired,
            "near_exhausted": a.group_ends_exhausted,
            "carrier_stopped": a.group_carriers_stopped,
            "stopped_short": a.group_stopped_short,
            "replanned": a.group_replans,
            "rejected_at_issue": a.group_orders_rejected,
            "members_left_out": a.group_members_left_out,
            "carrier_failures": ROUTE_FAILURES
                .iter()
                .zip(a.group_failures)
                .map(|(name, n)| ((*name).to_owned(), json!(n)))
                .collect::<serde_json::Map<String, Value>>(),
        },
        "expired_without_planner": a.expired_without_planner,
        "group_wait_renewals": a.group_wait_renewals,
        "group_pool_work": a.group_pool_work,
        "detours_declined": a.detours_declined,
        "detours_without_fast": a.detours_without_fast,
        "detours_at_capacity": a.detours_at_capacity,
        "samples_1_4ki": a.samples,
    })
}

/// Cumulative whole-population passes: the simulation's and the server's unit
/// frame. Each is `{calls, visited}`; later phases gate `visited` per tick at 0.
pub(crate) fn passes_json(
    passes: &[(&'static str, aoe_simulation::PassCount)],
    unit_frames: (u64, u64),
) -> Value {
    let mut out = serde_json::Map::new();
    for (name, count) in passes {
        out.insert(
            (*name).to_owned(),
            json!({"calls": count.calls, "visited": count.visited}),
        );
    }
    out.insert(
        "unit_frame".to_owned(),
        json!({"calls": unit_frames.0, "visited": unit_frames.1}),
    );
    Value::Object(out)
}

pub(crate) fn scheduler_json(stats: &RouteSchedulerStats, tick: u64) -> Value {
    let m = &stats.metrics;
    json!({
        "tick": tick,
        "planners": stats.planners, "searches": stats.searches,
        "waiting": stats.waiting_units,
        "waiting_without_planner": stats.waiting_without_planner,
        "waiting_players": stats.waiting_players,
        "oldest_wait_ticks": stats.oldest_wait_ticks,
        "players_at_search_cap": stats.players_at_search_cap,
        "most_waiting": stats.most_waiting,
        "most_waiting_player": stats.most_waiting_player.map(|p| p.0),
        "orders": m.orders, "fast_work": m.issue_work, "fair_work": m.fair_work,
        "lookahead_work": m.lookahead_work, "found": m.found, "failed": m.failed,
        "expired": m.expired, "stuck": m.stuck, "slot_refusals": m.slot_refusals,
        "attribution": attribution_json(&m.attribution),
    })
}

async fn scheduler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    let service = state.gameplay.read().await.clone();
    let world = service.world.read().await;
    let humans: Vec<PlayerId> = service
        .slots
        .iter()
        .filter(|s| s.human)
        .map(|s| s.player)
        .collect();
    let mut answer = scheduler_json(&world.route_scheduler_stats(), world.tick().0);
    answer["units"] = json!(world.units().count());
    answer["movers"] = json!(world.units().filter(|u| u.moving || u.planning).count());
    answer["order_demand"] = json!({
        "reassignment_seconds": 10,
        "issuers": (["player", "python_ai", "auto_scout", "steward", "vassal", "general", "rally_automation"]
            .into_iter().zip(world.order_demand()).collect::<BTreeMap<_, _>>())
    });
    answer["queue"] = queue_json(&world.route_queue_breakdown(), &humans);
    answer["passes"] = passes_json(&world.pass_counts(), service.unit_frame_passes());
    let (converted, changed) = service.unit_frame_work();
    answer["passes"]["unit_frame_converted"] = json!({"visited": converted});
    answer["passes"]["unit_frame_changed"] = json!({"visited": changed});
    Ok(Json(answer))
}

/// The queue breakdown: by requester side (human or AI seat), by kind and
/// reach, wait percentiles in ticks per kind, and the busiest players.
pub(crate) fn queue_json(queue: &RouteQueueBreakdown, humans: &[PlayerId]) -> Value {
    let pick = |waits: &[u64], percent: usize| {
        waits
            .get((waits.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
    };
    let (mut cells, mut kinds, mut players) = (serde_json::Map::new(), Vec::new(), BTreeMap::new());
    let mut sides = [[0usize; 2]; 2];
    for (k, kind) in ROUTE_REQUESTERS.iter().enumerate() {
        let mut waits = Vec::new();
        let (mut active, mut reach) = (0, [0usize; 3]);
        for ((player, requester, r), cell) in &queue.cells {
            if *requester != k {
                continue;
            }
            let side = usize::from(!humans.contains(player));
            sides[side][0] += cell.waits.len();
            sides[side][1] += cell.active;
            *players.entry(player.0).or_insert(0usize) += cell.waits.len();
            waits.extend_from_slice(&cell.waits);
            active += cell.active;
            reach[*r] += cell.waits.len();
            let key = format!("{}/{kind}/{}", ["human", "ai"][side], ROUTE_REACH[*r]);
            let entry = cells
                .entry(key)
                .or_insert(json!({"waiting": 0, "active": 0}));
            entry["waiting"] =
                json!(entry["waiting"].as_u64().unwrap_or(0) + cell.waits.len() as u64);
            entry["active"] = json!(entry["active"].as_u64().unwrap_or(0) + cell.active as u64);
        }
        waits.sort_unstable();
        kinds.push((
            kind.to_string(),
            json!({
                "waiting": waits.len(), "active": active,
                "near": reach[0], "far": reach[1], "unplanned": reach[2],
                "wait_p50": pick(&waits, 50), "wait_p90": pick(&waits, 90),
                "wait_p99": pick(&waits, 99), "wait_max": waits.last(),
            }),
        ));
    }
    let mut busiest: Vec<(u16, usize)> = players.into_iter().filter(|(_, n)| *n > 0).collect();
    busiest.sort_by_key(|(player, n)| (std::cmp::Reverse(*n), *player));
    busiest.truncate(5);
    json!({
        "tick": queue.tick,
        "human": {"waiting": sides[0][0], "active": sides[0][1]},
        "ai": {"waiting": sides[1][0], "active": sides[1][1]},
        "by_kind": kinds.into_iter().collect::<serde_json::Map<String, Value>>(),
        "by_side_kind_reach": cells,
        "busiest_waiting_players": busiest,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRequest {
    player: u16,
    units: Vec<u32>,
    /// Destination tile.
    to: [i32; 2],
    #[serde(default)]
    formation: Formation,
}

fn length_tiles(tiles: &[TileCoord]) -> f64 {
    tiles
        .windows(2)
        .map(|p| f64::from(p[1].x - p[0].x).hypot(f64::from(p[1].y - p[0].y)))
        .sum()
}

async fn route_probe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProbeRequest>,
) -> Result<Json<Value>, Error> {
    authorize(&state, &headers)?;
    let service = state.gameplay.read().await.clone();
    let ids: Vec<EntityId> = request.units.iter().copied().map(EntityId).collect();
    let group = {
        let world = service.world.read().await;
        world
            .probe_group(PlayerId(request.player), &ids, request.formation)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
    };
    let (kind, (speed, denominator)) = group.slowest;
    let origin = group.origin.tile_floor();
    let to = TileCoord::new(request.to[0], request.to[1]);
    let mut planner = RoutePlanner::new(origin, to, MAX_ROUTE_PLANNER_WORK);
    let mut tiles = vec![origin];
    let (mut legs, started) = (0_u32, Instant::now());
    let mut leg_starts = Vec::new();
    let outcome = loop {
        let poll = {
            let world = service.world.read().await;
            world.probe_route_poll(&mut planner, PROBE_POLL_WORK, kind)
        };
        match poll {
            RoutePlannerPoll::Path(path) => {
                legs += 1;
                leg_starts.push((path.tiles.first().copied(), tiles.last().copied()));
                for tile in path.tiles {
                    if tiles.last() != Some(&tile) {
                        tiles.push(tile);
                    }
                }
                if planner.is_terminal() && !planner.has_route_continuation() {
                    break "path".to_owned();
                }
            }
            RoutePlannerPoll::Pending => {}
            other => break format!("{other:?}"),
        }
        if started.elapsed() > PROBE_DEADLINE {
            break "probe_deadline".to_owned();
        }
        tokio::task::yield_now().await;
    };
    let certified = planner.certified_destination();
    // Route sanity: a leg must start where the previous one ended, and a
    // shortest route never visits a tile twice.
    let disjoint_legs = leg_starts
        .iter()
        .filter(|(first, last)| first != last)
        .count();
    let mut seen = std::collections::HashSet::new();
    let revisits = tiles.iter().filter(|t| !seen.insert(**t)).count();
    let max_step = tiles
        .windows(2)
        .map(|p| (p[1].x - p[0].x).abs().max((p[1].y - p[0].y).abs()))
        .max()
        .unwrap_or(0);
    let sampled: Vec<[i32; 2]> = tiles.iter().step_by(25).map(|t| [t.x, t.y]).collect();
    let length = length_tiles(&tiles);
    let per_tick = |(s, d): (u64, u64)| s as f64 / d.max(1) as f64 / 1024.0;
    let reachable = outcome == "path" && certified.is_none() && tiles.last() == Some(&to);
    let tile_size = f64::from(aoe_core::FIXED_SUBUNITS_PER_TILE);
    Ok(Json(json!({
        "origin": [origin.x, origin.y],
        "destination": [to.x, to.y],
        "outcome": outcome,
        "reachable": reachable,
        "certified": certified.map(|t| [t.x, t.y]),
        "end": tiles.last().map(|t| [t.x, t.y]),
        "legs": legs,
        "disjoint_legs": disjoint_legs,
        "revisits": revisits,
        "max_step": max_step,
        "sampled_route": sampled,
        "route_points": tiles.len(),
        "length_tiles": length,
        "straight_tiles": f64::from(to.x - origin.x).hypot(f64::from(to.y - origin.y)),
        "work": planner.work(),
        "probe_seconds": started.elapsed().as_secs_f64(),
        "slowest_kind": format!("{kind:?}"),
        "slowest_tiles_per_tick": per_tick((speed, denominator)),
        "speeds_tiles_per_tick": group.speeds.iter()
            .map(|(k, s)| (format!("{k:?}"), json!(per_tick(*s))))
            .collect::<serde_json::Map<_, _>>(),
        "extent_tiles": group.extent as f64 / tile_size,
        "ideal_ticks": (length / per_tick((speed, denominator)).max(1e-9)).ceil(),
    })))
}
