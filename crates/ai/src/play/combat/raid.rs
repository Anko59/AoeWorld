//! Recall the field force after damage to a settlement or worker
//! (`ai/aoe_ai/play/combat/raid.py`).
use std::collections::BTreeSet;

use aoe_sdk::Event;

use super::CombatManager;
use super::march::{Purpose, march};
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{dist, near_any, nearest};
use crate::view::{Tile, Unit};

/// Ticks of `seconds` game seconds.
fn ticks(ctx: &Context<'_>, seconds: u64) -> i64 {
    (seconds * ctx.hz()) as i64
}

pub fn observe(ctx: &mut Context<'_>) {
    let cfg = ctx.config;
    // (target, attacker, is a building, tile) of every hit, oldest first.
    let hits: Vec<(u32, Option<u32>, bool, Tile)> = ctx
        .events
        .iter()
        .filter_map(|event| match event {
            Event::UnderAttack {
                target,
                attacker,
                building,
                tile,
            } => Some((target.0, attacker.map(|a| a.0), *building, (tile.x, tile.y))),
            _ => None,
        })
        .collect();
    let Some(last) = hits.last() else {
        return;
    };
    ctx.board.bump_by("threats_seen", hits.len() as u64);
    ctx.board.threat_until = ctx.tick as i64 + ticks(ctx, cfg.threat_seconds);
    ctx.board.threat_tile = Some(last.3);

    let workers: BTreeSet<u32> = ctx.villagers.iter().map(|u| u.id).collect();
    let wildlife: BTreeSet<u32> = ctx.world.animals().iter().map(|a| a.id).collect();
    for (target, attacker, building, tile) in hits.into_iter().rev() {
        let enemy = attacker.and_then(|a| ctx.enemies.iter().find(|e| e.id == a).cloned());
        if !building
            && (!workers.contains(&target) || attacker.is_none_or(|a| wildlife.contains(&a)))
        {
            continue;
        }
        begin(ctx, tile, enemy.as_ref());
        return;
    }
}

/// Start, or extend, the defence of the place `tile` where we were hit.
pub fn begin(ctx: &mut Context<'_>, tile: Tile, enemy: Option<&Unit>) {
    let cfg = ctx.config;
    let tick = ctx.tick as i64;
    let hold = ticks(ctx, cfg.raid_hold_seconds);
    let counter = ticks(ctx, cfg.counter_seconds);
    let home = ctx.home;
    let board = &mut *ctx.board;
    let fresh = board
        .raid_tile
        .is_none_or(|at| board.raid_until <= tick || dist(at, tile) > f64::from(cfg.defend_radius));
    if fresh {
        board.raid_started = tick;
        board.raid_tile = Some(tile);
        board.attacking = false;
        board.attack_members.clear();
        board.attack_target = None;
        board.march_goal = None;
        board.search_index = 0;
        board.counter_owner = None;
        board.counter_bearing = None;
        board.raid_reach.clear();
    }
    board.raid_until = tick + hold;
    if let Some(enemy) = enemy {
        board.counter_owner = Some(enemy.owner);
        if board.counter_bearing.is_none()
            && let Some(home) = home
        {
            board.counter_bearing = Some((enemy.x - home.0, enemy.y - home.1));
        }
        board.counter_until = board.raid_until + counter;
    }
}

/// Defend any of our buildings, camps and mills included, against armed
/// enemies standing at it, before they have damaged anything.
///
/// Checked at a fixed interval per bot; one grid of our buildings answers every
/// enemy, so the cost does not grow with soldiers or with the enemy count.
pub fn watch_sites(ctx: &mut Context<'_>) {
    let cfg = ctx.config;
    let Some(home) = ctx.home else {
        return;
    };
    if ctx.board.raid_until > ctx.tick as i64
        || ctx.enemies.is_empty()
        || ctx.board.cooling("site-watch", ctx.tick)
    {
        return;
    }
    let wait = cfg.site_watch_seconds * ctx.hz();
    ctx.board.cool("site-watch", ctx.tick, wait);
    let sites: Vec<Tile> = ctx.buildings.iter().map(|b| b.center()).collect();
    let near_site = near_any(&sites, f64::from(cfg.site_guard_tiles));
    // The base itself is `defend`'s ground; this covers camps and outposts.
    let threats: Vec<Unit> = ctx
        .enemies
        .iter()
        .filter(|e| {
            e.alive
                && e.military
                && near_site(e.tile())
                && dist(e.tile(), home) > f64::from(cfg.defend_radius)
        })
        .cloned()
        .collect();
    let Some(closest) = nearest(&threats, home, Unit::tile) else {
        return;
    };
    let saved = ctx.board.raid_tile;
    ctx.board.raid_tile = Some(closest.tile());
    let found = reachable(ctx, &threats);
    let Some(enemy) = nearest(&found, home, Unit::tile) else {
        ctx.board.raid_tile = saved;
        return;
    };
    ctx.board.threat_until = ctx.tick as i64 + ticks(ctx, cfg.threat_seconds);
    ctx.board.bump("sites_threatened");
    begin(ctx, enemy.tile(), Some(enemy));
}

/// Raiders the settlement's soldiers can walk to, cached per raid.
///
/// An enemy across water never renews the raid, so it cannot pin the army in
/// defence. Unanswered queries (budget spent) count as unknown, not reachable.
pub fn reachable(ctx: &mut Context<'_>, visible: &[Unit]) -> Vec<Unit> {
    let home = ctx.home.unwrap_or_default();
    if ctx.board.raid_reach.len() > 256 {
        ctx.board.raid_reach.clear();
    }
    let at = ctx.board.raid_tile.unwrap_or(home);
    let mut shortlist: Vec<&Unit> = visible.iter().collect();
    shortlist.sort_by(|a, b| {
        dist(a.tile(), at)
            .total_cmp(&dist(b.tile(), at))
            .then(a.id.cmp(&b.id))
    });
    let mut budget = (ctx.config.land_queries_per_decision / 4).max(1);
    let mut found = Vec::new();
    for enemy in shortlist {
        if !ctx.board.raid_reach.contains_key(&enemy.id) {
            if budget == 0 {
                continue;
            }
            budget -= 1;
            let route = ctx.land_route(home, enemy.tile());
            if route.is_none() && ctx.land_unanswered(home, enemy.tile()) {
                continue;
            }
            ctx.board.raid_reach.insert(enemy.id, route.is_some());
        }
        if ctx.board.raid_reach[&enemy.id] {
            found.push(enemy.clone());
        }
    }
    found
}

pub fn respond(
    ctx: &mut Context<'_>,
    army: &[Unit],
    manager: &CombatManager,
) -> Result<bool, OrderBudgetExhausted> {
    let cfg = ctx.config;
    let Some(home) = ctx.home else {
        return Ok(false);
    };
    if ctx.board.raid_until <= ctx.tick as i64 {
        return Ok(false);
    }
    let Some(at) = ctx.board.raid_tile else {
        return Ok(false);
    };
    // Stay near the hit while raiders remain visible around the settlement.
    let visible: Vec<Unit> = ctx
        .enemies
        .iter()
        .filter(|e| e.alive && dist(e.tile(), at) <= f64::from(cfg.defend_radius))
        .cloned()
        .collect();
    let raiders = reachable(ctx, &visible);
    if !raiders.is_empty() {
        let until = ctx.tick as i64 + ticks(ctx, cfg.raid_hold_seconds);
        ctx.board.raid_until = until;
        if raiders
            .iter()
            .any(|e| Some(e.owner) == ctx.board.counter_owner)
        {
            ctx.board.counter_until = until + ticks(ctx, cfg.counter_seconds);
        }
    }
    if !raiders.is_empty() && visible.len() >= 3 && army.len() * 2 < visible.len() {
        let shelter = ctx
            .centers
            .iter()
            .copied()
            .min_by(|a, b| dist(*a, at).total_cmp(&dist(*b, at)))
            .unwrap_or(home);
        manager.pull_back(ctx, &visible, Some(shelter), 4)?;
    }
    let local: Vec<Unit> = army
        .iter()
        .filter(|u| dist(u.tile(), at) <= f64::from(cfg.defend_radius))
        .cloned()
        .collect();
    manager.engage(ctx, &local, &raiders)?;
    let goal = nearest(&raiders, at, Unit::tile).map_or(at, Unit::tile);
    let contact = if raiders.is_empty() {
        f64::from(cfg.defend_radius / 2)
    } else {
        f64::from(cfg.combat_engage_tiles)
    };
    let tiles: Vec<Tile> = raiders.iter().map(Unit::tile).collect();
    let nearby = near_any(&tiles, f64::from(cfg.combat_engage_tiles));
    // Troops already fighting the raiders stay in that fight; distant troops
    // may abandon an unrelated march or attack on the first recall hop.
    let remote: Vec<Unit> = army
        .iter()
        .filter(|u| {
            dist(u.tile(), goal) > contact && (!nearby(u.tile()) || u.attack_target.is_none())
        })
        .cloned()
        .collect();
    march(ctx, &remote, home, goal, Purpose::Recall)?;
    ctx.board.bump("raid_defences");
    Ok(true)
}
