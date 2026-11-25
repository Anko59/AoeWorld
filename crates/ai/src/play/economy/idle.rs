//! Giving work to idle villagers, and what to do when nothing is known to work.
use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{EntityId, Order, TileCoord};

use super::{
    ASSIGN_PAUSE_SECONDS, Done, MAX_ASSIGN_PER_DECISION, counts, nearest_center, send, shares,
    since, unemployed,
};
use crate::context::Context;
use crate::geometry::{clamp_tile, ring_points};
use crate::view::Unit;

/// Put every idle villager to work, a bounded number per decision, the least
/// served resource first.
pub fn assign_idle(ctx: &mut Context<'_>) -> Done {
    let mut idle: Vec<(u32, Unit)> = Vec::new();
    let mut builders: Vec<(u32, Unit)> = Vec::new();
    let mut steward_idle: BTreeMap<u32, (u32, u64)> = BTreeMap::new();
    let timeout = ctx.config.steward_idle_timeout_seconds * ctx.hz();
    for v in &ctx.villagers {
        if ctx.reserved_workers.contains(&v.id) || ctx.busy.contains(&v.id) || !unemployed(v) {
            continue;
        }
        let was_builder = ctx.board.builders.remove(&v.id).is_some();
        if let (Some(steward), true) = (v.managed_by, ctx.config.defer_to_steward) {
            let (previous, since_tick) = ctx
                .board
                .steward_idle
                .get(&v.id)
                .copied()
                .unwrap_or((steward, ctx.tick));
            let first = if previous == steward {
                since_tick
            } else {
                ctx.tick
            };
            steward_idle.insert(v.id, (steward, first));
            if ctx.tick.saturating_sub(first) < timeout {
                continue;
            }
        }
        if was_builder {
            builders.push((v.id, v.clone()));
        } else {
            idle.push((v.id, v.clone()));
        }
    }
    idle.extend(builders);
    ctx.board.steward_idle = steward_idle;
    if idle.is_empty() {
        return Ok(());
    }
    let mut counts = counts(ctx);
    let shares = shares(ctx);
    let total = ctx.villagers.len().max(1) as f64;
    let pause = ASSIGN_PAUSE_SECONDS * ctx.hz();
    let alive: BTreeSet<u32> = ctx.villagers.iter().map(|v| v.id).collect();
    ctx.board.assigned.retain(|id, _| alive.contains(id));
    learn_unreachable(ctx, idle.iter().map(|(_, worker)| worker));
    if ctx.late_economy {
        idle.sort_by_key(|(id, _)| *id);
    }
    let start = if ctx.late_economy {
        let after = ctx.board.idle_after + 1;
        idle.partition_point(|(id, _)| i64::from(*id) < after)
    } else {
        0
    };
    let limit = ctx.land_query_limit;
    let result = give_jobs(ctx, &idle, start, &mut counts, &shares, total, pause);
    ctx.land_query_limit = limit;
    result
}

/// The loop of `assign_idle`, whose land-query limit the caller restores.
fn give_jobs(
    ctx: &mut Context<'_>,
    idle: &[(u32, Unit)],
    start: usize,
    counts: &mut [u32; 4],
    shares: &[f64; 4],
    total: f64,
    pause: u64,
) -> Done {
    let (mut given, limit) = (0, ctx.land_query_limit);
    let large_roster = ctx.late_economy && idle.len() > MAX_ASSIGN_PER_DECISION;
    let quantum = ctx.config.idle_worker_land_queries.min((limit / 2).max(1));
    for offset in 0..idle.len() {
        let (worker_id, villager) = &idle[(start + offset) % idle.len()];
        let worker_id = *worker_id;
        let rested = match ctx.board.assigned.get(&worker_id) {
            Some(tick) => since(ctx.tick, *tick) >= pause as i64,
            None => true,
        };
        if !rested {
            continue;
        }
        if given >= MAX_ASSIGN_PER_DECISION
            || (ctx.late_economy && !large_roster && given > 0 && ctx.land_budget_exhausted())
        {
            break;
        }
        ctx.require_order_budget()?;
        ctx.board.idle_after = i64::from(worker_id);
        ctx.board.assigned.insert(worker_id, ctx.tick);
        given += 1;
        if large_roster {
            ctx.land_query_limit = limit.min(ctx.land_queries() + quantum);
        }
        let mut order = [0usize, 1, 2, 3];
        order.sort_by(|a, b| {
            let gap = |k: usize| shares[k] * total - f64::from(counts[k]);
            gap(*b).total_cmp(&gap(*a))
        });
        let mut sent = false;
        for key in order {
            if shares[key] <= 0.0 && key != order[0] {
                continue;
            }
            if send(ctx, villager, key)? {
                counts[key] += 1;
                sent = true;
                break;
            }
        }
        if !sent {
            explore(ctx, villager)?;
        }
    }
    Ok(())
}

/// A villager idle right after a gather order could not reach the node: leave
/// that node alone for a while instead of sending villagers there again (each
/// such trip costs the server a hopeless route search).
fn learn_unreachable<'a>(ctx: &mut Context<'_>, idle: impl Iterator<Item = &'a Unit>) {
    let alive: BTreeSet<u32> = ctx.villagers.iter().map(|v| v.id).collect();
    ctx.board.gather_sent.retain(|id, _| alive.contains(id));
    let hz = ctx.hz();
    let window = (ctx.config.gather_fail_seconds * hz) as i64;
    let settle = (2 * hz) as i64;
    let wait = ctx.config.gather_unreachable_seconds * hz;
    for villager in idle {
        let Some(last) = ctx.board.gather_sent.get(&villager.id).copied() else {
            continue;
        };
        let age = since(ctx.tick, last.1);
        if age < settle {
            continue;
        }
        ctx.board.gather_sent.remove(&villager.id);
        if age > window {
            continue;
        }
        ctx.board
            .cool(&format!("unreachable:{}:villager", last.0), ctx.tick, wait);
        ctx.board.bump("gather_unreachable");
    }
}

/// Nothing known to work: walk a ring around home to uncover something.
fn explore(ctx: &mut Context<'_>, villager: &Unit) -> Done {
    ctx.board.bump("explored");
    let explored = ctx.board.stats.get("explored").copied().unwrap_or(0);
    let ring = 12 + 6 * (explored / 8) as i32;
    let Some(origin) = nearest_center(ctx, villager.tile()) else {
        return Ok(());
    };
    let points = ring_points(origin, ring.min(60), 8);
    let goal = clamp_tile(
        points[(explored % points.len() as u64) as usize],
        ctx.world.map_size(),
        3,
    );
    let routed = ctx
        .land_route(villager.tile(), goal)
        .is_some_and(|route| !route.is_empty());
    let target = if routed {
        Some(goal)
    } else {
        ctx.land_frontier(villager.tile(), goal)
    };
    if let Some(target) = target {
        let target = ctx.land_hop(villager.tile(), target);
        if target != villager.tile() {
            let order = Order::Move {
                unit: EntityId(villager.id),
                to: TileCoord::new(target.0, target.1),
            };
            ctx.order_or_stop("move", order)?;
        }
    }
    Ok(())
}
