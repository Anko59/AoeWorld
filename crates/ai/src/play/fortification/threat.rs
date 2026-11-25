//! Local defensive spending from public enemies and recent damage and losses
//! (`ai/aoe_ai/play/fortification/threat.py`).
use aoe_sdk::Event;

use crate::context::Context;
use crate::view::Tile;

/// Whether enemies, enemy buildings or recent alerts are near `tile`: enemy
/// buildings mark a frontier and count within the wide radius, units and
/// alerts within the smaller local danger radius. No route or site queries and
/// no orders.
pub fn threatened(ctx: &Context<'_>, tile: Tile) -> bool {
    let config = ctx.config;
    if !config.fortifications {
        return false;
    }
    let reaches = |point: (f64, f64), radius: i32| {
        let (dx, dy) = (f64::from(tile.0) - point.0, f64::from(tile.1) - point.1);
        (dx * dx + dy * dy).sqrt() <= f64::from(radius)
    };
    let cell = f64::from(config.fort_alert_cell);
    let alerts = ctx
        .board
        .defence_alerts
        .iter()
        .filter(|(_, until)| **until > ctx.tick)
        .map(|(c, _)| ((f64::from(c.0) + 0.5) * cell, (f64::from(c.1) + 0.5) * cell));
    let local = ctx
        .enemies
        .iter()
        .map(|u| (f64::from(u.x), f64::from(u.y)))
        .chain(alerts)
        .any(|point| reaches(point, config.fort_threat_radius));
    local
        || ctx.world.known_enemy_buildings().iter().any(|b| {
            let c = b.center();
            reaches(
                (f64::from(c.0), f64::from(c.1)),
                config.fort_frontier_radius,
            )
        })
}

/// Remember where our units and buildings fell or were hit. One alert is kept
/// per local cell; distant army losses cannot authorize home defences, and a
/// fixed cap bounds long-game memory (the oldest alert goes first, the lowest
/// cell among equals).
pub fn observe(ctx: &mut Context<'_>) {
    let (tick, config, events) = (ctx.tick, ctx.config, ctx.events);
    let wait = config.fort_threat_seconds * u64::from(ctx.world.tick_hz());
    let board = &mut *ctx.board;
    board.defence_alerts.retain(|_, until| *until > tick);
    for event in events {
        let tile = match event {
            Event::UnderAttack { tile, .. } => Some((tile.x, tile.y)),
            Event::UnitDied {
                mine: true, tile, ..
            } => Some((tile.x, tile.y)),
            Event::BuildingDestroyed { mine: true, id, .. } => {
                board.defence_buildings.get(&id.0).copied()
            }
            _ => None,
        };
        let Some(tile) = tile else { continue };
        let cell = (
            tile.0.div_euclid(config.fort_alert_cell),
            tile.1.div_euclid(config.fort_alert_cell),
        );
        if !board.defence_alerts.contains_key(&cell)
            && board.defence_alerts.len() >= config.fort_alert_limit
        {
            let oldest = board
                .defence_alerts
                .iter()
                .min_by_key(|(_, until)| **until)
                .map(|(cell, _)| *cell);
            if let Some(oldest) = oldest {
                board.defence_alerts.remove(&oldest);
            }
        }
        board.defence_alerts.insert(cell, tick + wait);
    }
    board.defence_buildings = ctx.buildings.iter().map(|b| (b.id, b.center())).collect();
}
