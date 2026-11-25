//! Local defensive spending from public enemies and recent damage and losses
//! (`ai/aoe_ai/play/fortification/threat.py`).
use aoe_sdk::Event;

use crate::context::Context;

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
