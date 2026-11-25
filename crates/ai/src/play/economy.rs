//! Villager production and the balance between food, wood, gold and stone
//! (`ai/aoe_ai/play/economy.py`).
mod food;
mod idle;
mod shares;

use aoe_sdk::{BuildingKind, EntityId, Order, ResourceKind, Tech, UnitTask};

use super::buildorder;
use super::constructor;
use super::naval::food_reachable;
use super::production::train_villagers;
use super::reseeding::queue_reseeds;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{dist, dist2, nearest};
use crate::roster::{building_price, research_price};
use crate::view::{Node, Tile, Unit};

pub use shares::{counts, shares};

type Done<T = ()> = Result<T, OrderBudgetExhausted>;

/// The resource keys in the order of Python's `KEYS`.
pub const FOOD: usize = 0;
pub const WOOD: usize = 1;
pub const GOLD: usize = 2;
pub const STONE: usize = 3;
const KINDS: [ResourceKind; 4] = [
    ResourceKind::Food,
    ResourceKind::Wood,
    ResourceKind::Gold,
    ResourceKind::Stone,
];
const CAMPS: [BuildingKind; 4] = [
    BuildingKind::Mill,
    BuildingKind::LumberCamp,
    BuildingKind::MiningCamp,
    BuildingKind::MiningCamp,
];
/// Most idle villagers dispatched in one decision (spreads the order burst).
const MAX_ASSIGN_PER_DECISION: usize = 16;
/// Seconds before a villager that fell idle again is given another job.
const ASSIGN_PAUSE_SECONDS: u64 = 4;
/// Hunt orders on one animal before it is written off.
const HUNT_TRIES: u32 = 12;
/// Seconds a hunted animal may stay unchanged before its hunters are stood down.
const HUNT_STALL_SECONDS: u64 = 40;

/// Failed jobs and completed builders can leave a worker holding a load.
///
/// A live gather job waiting for a drop-off still exposes its resource kind.
/// Reassign only orphaned loads, preserving ordinary return/drop-off retries.
pub fn unemployed(villager: &Unit) -> bool {
    villager.alive
        && (villager.idle
            || (villager.task == UnitTask::Carrying
                && !villager.moving
                && villager.gathering.is_none()))
}

/// Ticks since `tick`, negative when it lies ahead.
fn since(now: u64, tick: u64) -> i64 {
    now as i64 - tick as i64
}

/// Whether the villager's last job is at least `pause` ticks old (never
/// assigned counts as long ago).
fn rested(ctx: &Context<'_>, id: u32, pause: u64) -> bool {
    match ctx.board.assigned.get(&id) {
        Some(tick) => since(ctx.tick, *tick) >= pause as i64,
        None => true,
    }
}

/// The Town Center nearest to `tile`, else home.
fn nearest_center(ctx: &Context<'_>, tile: Tile) -> Option<Tile> {
    nearest(&ctx.centers, tile, |c| *c).copied().or(ctx.home)
}

/// Queue villagers (before houses are placed, so the margin sees them).
pub fn produce(ctx: &mut Context<'_>) -> Done {
    if ctx.economy.is_some() && ctx.home.is_some() {
        train_villagers(ctx)?;
    }
    Ok(())
}

/// Put idle villagers to work and repair imbalanced live jobs.
pub fn work(ctx: &mut Context<'_>) -> Done {
    if ctx.economy.is_none() || ctx.home.is_none() {
        return Ok(());
    }
    food::forget_farmers(ctx);
    food::drop_stuck_hunts(ctx)?;
    castle_food(ctx)?;
    idle::assign_idle(ctx)?;
    rebalance(ctx)?;
    queue_reseeds(ctx)
}

/// Reserve camp wood while the cutter travels or delivers its load. Build
/// beside the trees with an unloaded worker. The first Barracks has priority.
pub fn wood_dropoff(ctx: &mut Context<'_>) -> Done {
    if ctx.economy.is_none() || ctx.home.is_none() {
        return Ok(());
    }
    let kind = BuildingKind::LumberCamp;
    if ctx.board.cooling(&format!("camp:{kind:?}"), ctx.tick) {
        return Ok(());
    }
    if buildorder::unpaid_age_up_barracks(ctx) {
        return Ok(());
    }
    let defer = ctx.config.defer_to_steward;
    let workers: Vec<Unit> = ctx
        .villagers
        .iter()
        .filter(|v| {
            matches!(
                v.task,
                UnitTask::Gathering | UnitTask::Moving | UnitTask::Returning
            ) && v.gathering == Some(ResourceKind::Wood)
                && !ctx.busy.contains(&v.id)
                && !ctx.board.builders.contains_key(&v.id)
                && (v.managed_by.is_none() || !defer)
        })
        .cloned()
        .collect();
    if workers.is_empty() {
        return Ok(());
    }
    let worker = &workers[ctx.board.wood_dropoff_cursor % workers.len()];
    ctx.board.wood_dropoff_cursor = (ctx.board.wood_dropoff_cursor + 1) % workers.len();
    let sent = ctx.board.gather_sent.get(&worker.id).copied();
    let trees: Vec<Node> = ctx.resources(ResourceKind::Wood).to_vec();
    let mut node = sent.and_then(|(id, _)| trees.iter().find(|n| n.id == id && n.remaining > 0));
    let known_destination = node.is_some();
    let loaded = worker.carrying.is_some() || worker.task == UnitTask::Returning;
    if node.is_none() && !loaded {
        let live: Vec<&Node> = trees.iter().filter(|n| n.remaining > 0).collect();
        node = nearest(&live, worker.tile(), |n| n.tile()).copied();
    }
    let Some(node) = node else {
        return Ok(());
    };
    let cfg = ctx.config;
    if ctx.settlement_distance(node.tile()) > f64::from(cfg.wood_gather_distance)
        || ctx.dropoff_distance(node) <= f64::from(cfg.dropoff_distance)
        || constructor::camp_near(ctx, kind, node)
    {
        return Ok(());
    }
    let cost = building_price(kind);
    let can_build =
        !loaded && dist(node.tile(), worker.tile()) <= f64::from(cfg.wood_camp_contact_tiles);
    if can_build && ctx.can_afford(cost) {
        constructor::ensure_camp(ctx, kind, node, worker.id)?;
    } else if can_build || known_destination {
        ctx.reserve(cost);
    }
    Ok(())
}

/// Keep gold for the age and its first army upgrade before moving miners.
fn castle_food(ctx: &mut Context<'_>) -> Done {
    let Some(eco) = ctx.economy.clone() else {
        return Ok(());
    };
    let age = research_price(Tech::CastleAge);
    if !buildorder::wants_castle_food(ctx)
        || eco.food >= age[0]
        || eco.gold < age[2] + research_price(Tech::Crossbowman)[2]
    {
        return Ok(());
    }
    let pause = ctx.config.worker_dwell_seconds * ctx.hz();
    for worker in ctx.villagers.clone() {
        if worker.gathering == Some(ResourceKind::Gold)
            && worker.carrying.is_none()
            && !worker.moving
            && worker.task == UnitTask::Gathering
            && !ctx.busy.contains(&worker.id)
            && !ctx.board.builders.contains_key(&worker.id)
            && rested(ctx, worker.id, pause)
            && food::send_to_food(ctx, &worker)?
        {
            ctx.board.assigned.insert(worker.id, ctx.tick);
            ctx.board.bump("castle_food_workers");
            return Ok(());
        }
    }
    Ok(())
}

/// Rallied and reassigned workers need not fall idle to change resources.
///
/// Move at most one empty-handed worker, leaving a one-worker margin so
/// rounding and the next spawn cannot bounce workers between jobs.
fn rebalance(ctx: &mut Context<'_>) -> Done {
    let (counts, shares) = (counts(ctx), shares(ctx));
    let total = ctx.villagers.len().max(1) as f64;
    let gaps: [f64; 4] = std::array::from_fn(|k| shares[k] * total - f64::from(counts[k]));
    let source = (1..4).fold(0, |s, k| if gaps[k] < gaps[s] { k } else { s });
    let mut ranked = [FOOD, WOOD, GOLD, STONE];
    ranked.sort_by(|a, b| gaps[*b].total_cmp(&gaps[*a]));
    let mut target = None;
    for key in ranked {
        if gaps[key] > 1.0 && (key == FOOD || !ctx.resources(KINDS[key]).is_empty()) {
            target = Some(key);
            break;
        }
    }
    let Some(target) = target else {
        return Ok(());
    };
    if gaps[source] >= -1.0 {
        return Ok(());
    }
    let pause = ctx.config.worker_dwell_seconds * ctx.hz();
    let defer = ctx.config.defer_to_steward;
    for worker in ctx.villagers.clone() {
        if (worker.managed_by.is_some() && defer)
            || worker.gathering != Some(KINDS[source])
            || worker.carrying.is_some()
            || worker.moving
            || matches!(
                worker.task,
                UnitTask::Moving
                    | UnitTask::Returning
                    | UnitTask::Hunting
                    | UnitTask::Farming
                    | UnitTask::Building
                    | UnitTask::Seeding
            )
            || ctx.busy.contains(&worker.id)
            || ctx.reserved_workers.contains(&worker.id)
            || ctx.board.builders.contains_key(&worker.id)
            || ctx.board.farmers.values().any(|id| *id == worker.id)
            || !rested(ctx, worker.id, pause)
        {
            continue;
        }
        if send(ctx, &worker, target)? {
            ctx.board.assigned.insert(worker.id, ctx.tick);
            ctx.board.bump("workers_rebalanced");
            return Ok(());
        }
    }
    Ok(())
}

fn send(ctx: &mut Context<'_>, villager: &Unit, key: usize) -> Done<bool> {
    if key == FOOD {
        return food::send_to_food(ctx, villager);
    }
    let kind = KINDS[key];
    let Some(node) = best_node(ctx, villager, kind) else {
        return Ok(false);
    };
    if ctx.dropoff_distance(&node) > f64::from(ctx.config.dropoff_distance)
        && !(kind == ResourceKind::Wood && buildorder::unpaid_age_up_barracks(ctx))
        && constructor::ensure_camp(ctx, CAMPS[key], &node, villager.id)?
    {
        return Ok(true);
    }
    gather(ctx, villager, &node)?;
    Ok(true)
}

fn gather(ctx: &mut Context<'_>, villager: &Unit, node: &Node) -> Done {
    let order = Order::Gather {
        unit: EntityId(villager.id),
        resource: node.id,
    };
    ctx.order_or_stop("gather", order)?;
    ctx.board
        .gather_sent
        .insert(villager.id, (node.id, ctx.tick));
    Ok(())
}

fn best_node(ctx: &mut Context<'_>, villager: &Unit, kind: ResourceKind) -> Option<Node> {
    if ctx.late_economy && (ctx.land_budget_exhausted() || !ctx.world.land_known(villager.tile())) {
        return None;
    }
    let cfg = ctx.config;
    let reach = match kind {
        ResourceKind::Wood => cfg.wood_gather_distance,
        ResourceKind::Gold | ResourceKind::Stone => cfg.ore_gather_distance,
        ResourceKind::Food => cfg.max_gather_distance,
    };
    let mut nodes: Vec<Node> = ctx.resources(kind).to_vec();
    nodes.sort_by_key(|n| dist2(n.tile(), villager.tile()));
    nodes.truncate(48);
    let mut scored = Vec::new();
    for node in nodes {
        if ctx.settlement_distance(node.tile()) > f64::from(reach)
            || ctx
                .board
                .cooling(&format!("unreachable:{}:villager", node.id), ctx.tick)
        {
            continue;
        }
        let drop = ctx.dropoff_distance(&node).min(60.0);
        let score = 2.0 * drop + dist(node.tile(), villager.tile());
        scored.push((score, node));
    }
    scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
    scored
        .into_iter()
        .map(|(_, node)| node)
        .find(|node| food_reachable(ctx, villager, node))
}
