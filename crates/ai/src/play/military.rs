//! Compose an army from completed production buildings and rally it
//! (`ai/aoe_ai/play/military.py`).
use aoe_sdk::{EntityId, Order, RallyTarget, TileCoord, UnitKind, WorldPosition};

use super::buildorder;
use super::combat::march::{Purpose, march};
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{clamp_tile, dist, nearest};
use crate::roster::{
    best_recruit, land_producers, land_soldier, options, price, profile, visible_composition,
};
use crate::view::Tile;

type Done<T = ()> = Result<T, OrderBudgetExhausted>;

/// Queue soldiers at the finished production buildings, least represented line first.
pub fn train(ctx: &mut Context<'_>) -> Done {
    if ctx.board.cooling("world-limit:train", ctx.tick) {
        return Ok(());
    }
    let cfg = ctx.config;
    let defending = buildorder::emergency_defence(ctx);
    let minimum = if defending {
        cfg.emergency_army_villagers
    } else {
        cfg.army_min_villagers
    };
    let Some(home) = ctx.home else {
        return Ok(());
    };
    let Some(eco) = ctx.economy.clone() else {
        return Ok(());
    };
    if !ctx.feudal() || ctx.villagers.len() < minimum as usize {
        return Ok(());
    }
    // Castle Age planning reserves specific resources for development. The
    // Feudal opening can still pause the whole army for its age-up.
    if ctx.hold_military_training || (ctx.saving_castle && (!ctx.castle() || ctx.center.is_none()))
    {
        return Ok(());
    }
    if !ctx.castle()
        && ctx.villagers.len() >= cfg.castle_villagers as usize
        && buildorder::castle_requirement_kinds(ctx) < 2
        && !defending
    {
        return Ok(());
    }
    let trainers: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| land_producers().contains(&b.kind) && b.complete)
        .cloned()
        .collect();
    if trainers.is_empty() {
        return Ok(());
    }
    let start = ctx.board.military_cursor % trainers.len();
    let mut mix = cfg.army_mix.clone();
    match mix.iter_mut().find(|(kind, _)| *kind == UnitKind::Knight) {
        Some(entry) => entry.1 = cfg.castle_knight_weight,
        None => mix.push((UnitKind::Knight, cfg.castle_knight_weight)),
    }
    let mut have = ctx.combat_lines().clone();
    let enemies = visible_composition(&ctx.enemies, |kind| !profile(kind).naval);
    let known: Vec<_> = ctx
        .world
        .known_enemy_buildings()
        .into_iter()
        .filter(|b| !ctx.board.cooling(&format!("target:{}", b.id), ctx.tick))
        .collect();
    let target = known
        .iter()
        .find(|b| Some(b.id) == ctx.board.attack_target)
        .or_else(|| nearest(&known, home, |b| b.center()));
    let target_kind = target.map(|b| b.kind);
    for offset in 0..trainers.len() {
        if ctx.orders >= ctx.order_limit {
            return Ok(());
        }
        let index = (start + offset) % trainers.len();
        let building = &trainers[index];
        ctx.board.military_cursor = index + 1;
        if building.queue_length >= cfg.army_training_queue as usize
            || building.researching.is_some()
            || ctx.draining.contains(&building.id)
        {
            continue;
        }
        if ctx.free_population() <= 0 {
            return Ok(());
        }
        let roster = options(building.kind, eco.age, &eco.researched);
        let mut choices: Vec<UnitKind> = roster
            .iter()
            .copied()
            .filter(|kind| land_soldier(*kind))
            .collect();
        if !ctx.castle() {
            // The Feudal opening spends its early income on its core lines.
            choices.retain(|kind| mix.iter().any(|(line, _)| *line == profile(*kind).line));
            if ctx.board.blocked_units.contains("Spearman") && choices.contains(&UnitKind::Spearman)
            {
                choices.extend(
                    roster
                        .iter()
                        .copied()
                        .filter(|kind| profile(*kind).line == UnitKind::Militia),
                );
            }
        }
        let mut kind = best_recruit(ctx, &choices, &have, &enemies, target_kind, Some(&mix[..]));
        if kind.is_none()
            && !ctx.castle()
            && !choices.is_empty()
            && choices.iter().all(|unit| price(*unit)[1] > eco.wood)
        {
            // The Barracks can still train from food and gold while timber recovers.
            let infantry: Vec<UnitKind> = roster
                .iter()
                .copied()
                .filter(|unit| profile(*unit).line == UnitKind::Militia)
                .collect();
            kind = best_recruit(ctx, &infantry, &have, &enemies, target_kind, Some(&mix[..]));
        }
        let Some(kind) = kind else {
            continue;
        };
        ctx.spend(price(kind))?;
        let order = Order::Train {
            building: EntityId(building.id),
            kind,
        };
        ctx.order_or_stop("train", order)?;
        ctx.queued += 1;
        have.add(profile(kind).line, 1);
        ctx.board.bump("army_queued");
    }
    Ok(())
}

// ----- rally ----------------------------------------------------------------
/// Between the Town Center and the map centre: toward where trouble comes from.
fn rally_point(ctx: &Context<'_>, home: Tile) -> Tile {
    let size = ctx.world.map_size();
    let (cx, cy) = (size.0.div_euclid(2), size.1.div_euclid(2));
    let offset = f64::from(ctx.config.rally_offset);
    let d = dist(home, (cx, cy)).max(1.0);
    let toward =
        |from: i32, to: i32| from + (f64::from(to - from) / d * offset).round_ties_even() as i32;
    let point = (toward(home.0, cx), toward(home.1, cy));
    // Walk the point off anything the server refused as a destination.
    let bad = &ctx.board.bad_moves;
    for (dx, dy) in [
        (0, 0),
        (2, 0),
        (0, 2),
        (-2, 0),
        (0, -2),
        (3, 3),
        (-3, 3),
        (3, -3),
        (-3, -3),
        (5, 0),
        (0, 5),
        (-5, 0),
        (0, -5),
    ] {
        let candidate = clamp_tile((point.0 + dx, point.1 + dy), size, 3);
        if !bad.contains(&candidate) {
            return candidate;
        }
    }
    clamp_tile(point, size, 3)
}

/// Set one changed server-owned rally per decision, cycling over training sites.
fn rally_production(ctx: &mut Context<'_>) -> Done {
    let trainers: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| b.complete && land_producers().contains(&b.kind))
        .cloned()
        .collect();
    if trainers.is_empty() {
        return Ok(());
    }
    let cfg = ctx.config;
    for _ in 0..cfg.rally_scan_trainers.min(trainers.len()) {
        let i = ctx.board.military_rally_cursor % trainers.len();
        ctx.board.military_rally_cursor = i + 1;
        let trainer = &trainers[i];
        let key = format!("military-rally:{}", trainer.id);
        if ctx.board.cooling(&key, ctx.tick) {
            continue;
        }
        let Some(home) = nearest(&ctx.centers, trainer.center(), |t| *t)
            .copied()
            .or(ctx.home)
        else {
            continue;
        };
        let mut point = rally_point(ctx, home);
        if dist(trainer.center(), point) > f64::from(cfg.march_hop_tiles) {
            point = rally_point(ctx, trainer.center());
        }
        if trainer.rally_position == Some(point) {
            continue;
        }
        if ctx.land_route(trainer.center(), point).is_none() {
            return Ok(());
        }
        let Ok(position) = WorldPosition::from_tile_center(TileCoord::new(point.0, point.1)) else {
            return Ok(());
        };
        let order = Order::SetRally {
            building: EntityId(trainer.id),
            target: RallyTarget::Ground(position),
        };
        ctx.order_or_stop("set_rally", order)?;
        let wait = cfg.rally_retry_seconds * ctx.hz();
        ctx.board.cool(&key, ctx.tick, wait);
        return Ok(());
    }
    Ok(())
}

pub fn rally(ctx: &mut Context<'_>) -> Done {
    let Some(home) = ctx.home else {
        return Ok(());
    };
    rally_production(ctx)?;
    if ctx.board.raid_until > ctx.tick as i64 {
        return Ok(());
    }
    let point = rally_point(ctx, home);
    if ctx.board.attacking && ctx.board.attack_members.is_empty() {
        return Ok(());
    }
    let troops: Vec<_> = ctx
        .army
        .iter()
        .filter(|u| {
            u.alive && !ctx.board.attack_members.contains(&u.id) && dist(u.tile(), point) > 5.0
        })
        .cloned()
        .collect();
    march(ctx, &troops, home, point, Purpose::Rally)
}
