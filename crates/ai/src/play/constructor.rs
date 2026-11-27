//! Placing buildings: houses, camps, the mill and farms, and production
//! (`ai/aoe_ai/play/constructor.py`).
use std::collections::BTreeSet;

use aoe_sdk::{BuildingKind, EntityId, Order, ResourceKind, TileCoord, TileRect, UnitTask};

use super::buildorder;
use crate::blackboard::Plan;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{dist, nearest};
use crate::roster::building_price;
use crate::view::{Node, Tile, Unit};

type Done<T = ()> = Result<T, OrderBudgetExhausted>;

/// Seconds a foundation may sit without progress before a villager is sent to it.
const FOUNDATION_STALL_SECONDS: u64 = 8;

// ----- bookkeeping ----------------------------------------------------------
/// Forget plans that came true or went stale.
pub fn refresh(ctx: &mut Context<'_>) {
    let timeout = ctx.config.plan_timeout_seconds * ctx.hz();
    let tick = ctx.tick;
    let buildings = &ctx.buildings;
    ctx.board.plans.retain(|plan| {
        let seen = buildings.iter().any(|b| {
            b.kind == plan.kind && (b.x - plan.tile.0).abs() <= 1 && (b.y - plan.tile.1).abs() <= 1
        });
        !seen && tick.saturating_sub(plan.tick) < timeout
    });
}

/// Foundations of `kind` in progress or planned but not yet seen.
pub fn pending(ctx: &Context<'_>, kind: BuildingKind) -> usize {
    let unfinished = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == kind && !b.complete)
        .count();
    unfinished + ctx.board.plans.iter().filter(|p| p.kind == kind).count()
}

pub fn total(ctx: &Context<'_>, kind: BuildingKind) -> usize {
    buildorder::planned_count(ctx, kind)
}

// ----- placing --------------------------------------------------------------
/// Order `builders` to raise a `kind` near `near`; false if impossible now.
pub fn place(
    ctx: &mut Context<'_>,
    kind: BuildingKind,
    near: Tile,
    builders: &[u32],
    radius: Option<i32>,
    gap: i32,
) -> Done<bool> {
    if builders.is_empty() || ctx.board.cooling("world-limit:build", ctx.tick) {
        return Ok(false);
    }
    let cost = building_price(kind);
    if !ctx.can_afford(cost) {
        return Ok(false);
    }
    let radius = radius
        .filter(|r| *r != 0)
        .unwrap_or(ctx.config.build_search_radius);
    let mut site = None;
    // A cramped village (rivers, cliffs, forest) has no site close by: widen the search.
    for (reach, spacing) in [
        (radius, gap),
        (radius, 0),
        (2 * radius, gap),
        (3 * radius, 0),
    ] {
        if let Some(found) = ctx.find_build_site(kind, near, reach, spacing)
            && !ctx
                .board
                .bad_sites
                .contains(&(format!("{kind:?}"), found.0, found.1))
        {
            site = Some(found);
            break;
        }
    }
    let Some(site) = site else {
        ctx.board.bump(&format!("no_site:{kind:?}"));
        return Ok(false);
    };
    ctx.spend(cost)?;
    let order = Order::Build {
        builders: builders.iter().map(|id| EntityId(*id)).collect(),
        kind,
        origin: TileCoord::new(site.0, site.1),
    };
    ctx.order_or_stop("build", order)?;
    let tick = ctx.tick;
    ctx.board.plans.push(Plan {
        kind,
        tile: site,
        tick,
    });
    for id in builders {
        ctx.board.builders.insert(*id, (format!("{kind:?}"), site));
    }
    ctx.board.bump("build_orders");
    Ok(true)
}

/// The villager closest to `tile` that is not already building.
pub fn builder_near(ctx: &Context<'_>, tile: Tile) -> Option<Unit> {
    let farmers: BTreeSet<u32> = ctx.board.farmers.values().copied().collect();
    let free: Vec<&Unit> = ctx
        .villagers
        .iter()
        .filter(|v| {
            !matches!(
                v.task,
                UnitTask::Building | UnitTask::Seeding | UnitTask::Farming
            ) && !ctx.busy.contains(&v.id)
                && !ctx.reserved_workers.contains(&v.id)
                && !farmers.contains(&v.id)
        })
        .collect();
    nearest(&free, tile, |u| u.tile()).map(|u| (*u).clone())
}

/// A paid camp, including an unfinished or not-yet-visible one.
pub fn camp_near(ctx: &Context<'_>, kind: BuildingKind, node: &Node) -> bool {
    ctx.buildings
        .iter()
        .any(|b| b.kind == kind && dist(b.center(), node.tile()) <= 8.0)
        || ctx
            .board
            .plans
            .iter()
            .any(|p| p.kind == kind && dist(p.tile, node.tile()) <= 8.0)
}

/// A drop-off of `kind` close to `node`, unless one is on the way.
pub fn ensure_camp(
    ctx: &mut Context<'_>,
    kind: BuildingKind,
    node: &Node,
    builder: u32,
) -> Done<bool> {
    if camp_near(ctx, kind, node) {
        return Ok(false);
    }
    let key = format!("camp:{kind:?}");
    if ctx.board.cooling(&key, ctx.tick) {
        return Ok(false);
    }
    // Saving wood is not a failed site: do not cancel that budget's priority.
    if !ctx.can_afford(building_price(kind)) {
        return Ok(false);
    }
    let placed = place(ctx, kind, node.tile(), &[builder], Some(7), 0)?;
    if !placed {
        let wait = 4 * ctx.hz();
        ctx.board.cool(&key, ctx.tick, wait);
    }
    Ok(placed)
}

// ----- periodic needs -------------------------------------------------------
pub fn step(ctx: &mut Context<'_>) -> Done {
    if ctx.economy.is_none() || ctx.home.is_none() {
        return Ok(());
    }
    refresh(ctx);
    finish_foundations(ctx)?;
    age_requirements(ctx)?;
    production(ctx, None)
}

/// Restaff a stalled foundation when its footprint is clear.
fn finish_foundations(ctx: &mut Context<'_>) -> Done {
    let stall = FOUNDATION_STALL_SECONDS * ctx.hz();
    let foundations: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| !b.complete)
        .map(|b| {
            (
                b.id,
                b.progress,
                b.center(),
                b.kind,
                TileRect::from_xywh(b.x, b.y, b.width, b.height),
            )
        })
        .collect();
    let alive: BTreeSet<u32> = foundations.iter().map(|f| f.0).collect();
    let mut occupied = None;
    for (id, progress, center, kind, footprint) in foundations {
        let tick = ctx.tick;
        match ctx.board.progress.get(&id) {
            Some(last) if progress <= last.0 => {
                let key = format!("finish:{id}");
                if tick.saturating_sub(last.1) < stall || ctx.board.cooling(&key, tick) {
                    continue;
                }
                if !matches!(kind, BuildingKind::Farm | BuildingKind::Road) {
                    let occupied = occupied.get_or_insert_with(|| {
                        ctx.units
                            .iter()
                            .chain(&ctx.enemies)
                            .filter(|unit| unit.alive)
                            .map(Unit::tile)
                            .collect::<BTreeSet<_>>()
                    });
                    if (footprint.min.x..footprint.max.x).any(|x| {
                        (footprint.min.y..footprint.max.y).any(|y| occupied.contains(&(x, y)))
                    }) {
                        continue;
                    }
                }
                let Some(builder) = builder_near(ctx, center) else {
                    continue;
                };
                let order = Order::Construct {
                    builders: vec![EntityId(builder.id)],
                    building: EntityId(id),
                };
                ctx.order_or_stop("construct", order)?;
                ctx.board
                    .builders
                    .insert(builder.id, (format!("{kind:?}"), center));
                ctx.board.cool(&key, tick, stall);
                ctx.board.bump("foundations_resumed");
            }
            _ => {
                ctx.board.progress.insert(id, (progress, tick));
            }
        }
    }
    ctx.board.progress.retain(|id, _| alive.contains(id));
    Ok(())
}

/// Raise a house before the population cap is reached.
pub fn houses(ctx: &mut Context<'_>) -> Done {
    let Some(home) = ctx.home else {
        return Ok(());
    };
    if ctx.economy.is_none() {
        return Ok(());
    }
    let cfg = ctx.config;
    let production =
        ctx.count(BuildingKind::Barracks, false) + ctx.count(BuildingKind::ArcheryRange, false);
    let centers = ctx.count(BuildingKind::TownCenter, true) as i64;
    let margin = i64::from(cfg.house_margin) + 2 * production as i64 + 3 * (centers - 1).max(0);
    let houses = pending(ctx, BuildingKind::House) as i64;
    if ctx.free_population() + 5 * houses > margin {
        return Ok(());
    }
    if houses >= i64::from(cfg.house_pending) + (centers - 1) {
        return Ok(());
    }
    let near = ctx.centers.last().copied().unwrap_or(home);
    if let Some(builder) = builder_near(ctx, near)
        && place(
            ctx,
            BuildingKind::House,
            near,
            &[builder.id],
            None,
            cfg.building_gap,
        )?
    {
        ctx.board.bump("houses");
    }
    Ok(())
}

/// Mill and Lumber Camp early: drop-offs the economy needs and the buildings
/// Feudal Age asks for.
fn age_requirements(ctx: &mut Context<'_>) -> Done {
    let Some(home) = ctx.home else {
        return Ok(());
    };
    if ctx.villagers.len() < 8 || ctx.board.cooling("age-buildings", ctx.tick) {
        return Ok(());
    }
    for kind in [BuildingKind::LumberCamp, BuildingKind::Mill] {
        if total(ctx, kind) == 0 {
            let builder = builder_near(ctx, home);
            let tile = camp_site_hint(ctx, kind, home);
            let gap = ctx.config.building_gap;
            if let Some(builder) = builder
                && place(ctx, kind, tile, &[builder.id], Some(10), gap)?
            {
                let wait = 2 * ctx.hz();
                ctx.board.cool("age-buildings", ctx.tick, wait);
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Near the resource the camp will serve, else beside the Town Center.
fn camp_site_hint(ctx: &mut Context<'_>, kind: BuildingKind, home: Tile) -> Tile {
    let wanted = if kind == BuildingKind::LumberCamp {
        ResourceKind::Wood
    } else {
        ResourceKind::Food
    };
    let nodes = ctx.resources(wanted);
    nearest(nodes, home, |n| n.tile()).map_or(home, |n| n.tile())
}

/// Build the first market before spending on discretionary construction.
pub fn fund_market(ctx: &mut Context<'_>) -> Done {
    let market = BuildingKind::Market;
    if total(ctx, market) > 0 || !buildorder::wants_urgent_market(ctx) {
        return Ok(());
    }
    production(ctx, Some(market))?;
    if total(ctx, market) == 0 {
        let have = ctx.economy.as_ref().map_or(0, |e| i64::from(e.wood));
        let free = (have - i64::from(ctx.spent[1]) - i64::from(ctx.config.expansion_wood_float))
            .max(0) as u32;
        ctx.reserve([0, free.min(building_price(market)[1]), 0, 0]);
    }
    Ok(())
}

/// Raise the production buildings the build order asks for, one at a time.
pub fn production(ctx: &mut Context<'_>, only: Option<BuildingKind>) -> Done {
    if ctx.board.cooling("production", ctx.tick) {
        return Ok(());
    }
    let wants = match only {
        None => buildorder::wanted_buildings(ctx),
        Some(kind) => vec![(kind, 1)],
    };
    for (kind, wanted) in wants {
        if total(ctx, kind) >= wanted {
            continue;
        }
        if let Some(needs) = buildorder::prerequisite(kind)
            && ctx.count(needs, true) == 0
        {
            continue;
        }
        let cost = building_price(kind);
        let first_barracks = ctx.feudal()
            && kind == BuildingKind::Barracks
            && buildorder::unpaid_age_up_barracks(ctx);
        let margin = if first_barracks {
            0
        } else {
            ctx.config.wood_reserve
        };
        if !ctx.can_afford([cost[0], cost[1] + margin, cost[2], cost[3]]) {
            continue;
        }
        let near = if kind == BuildingKind::Monastery {
            super::relics::monastery_anchor(ctx)
        } else {
            nearest_trainer(ctx, kind).or(ctx.home)
        };
        let Some(near) = near else {
            continue;
        };
        let gap = ctx.config.building_gap;
        if let Some(builder) = builder_near(ctx, near)
            && place(ctx, kind, near, &[builder.id], None, gap)?
        {
            let wait = 3 * ctx.hz();
            ctx.board.cool("production", ctx.tick, wait);
            return Ok(());
        }
    }
    Ok(())
}

/// The finished Town Center with the fewest buildings of `kind` near it.
fn nearest_trainer(ctx: &Context<'_>, kind: BuildingKind) -> Option<Tile> {
    let mut best: Option<(usize, Tile)> = None;
    for center in ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::TownCenter && b.complete)
    {
        let crowd = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == kind && dist(center.center(), b.center()) < 24.0)
            .count();
        if best.is_none_or(|(least, _)| crowd < least) {
            best = Some((crowd, center.center()));
        }
    }
    best.map(|(_, tile)| tile)
}
