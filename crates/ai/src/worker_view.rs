//! Refresh the public view over large idle settlements away from the army
//! (`ai/aoe_ai/worker_view.py`).
use std::collections::BTreeMap;

use crate::context::Context;
use crate::play::economy::unemployed;
use crate::view::Tile;

pub fn focus_army(ctx: &mut Context<'_>, tile: Tile) {
    let config = ctx.config;
    let key = "army-view";
    if ctx.board.army_view_until != 0 || ctx.board.cooling(key, ctx.tick) {
        return;
    }
    let hz = ctx.hz();
    ctx.focus_view(Some(tile));
    ctx.board.army_view_until = (ctx.tick + config.worker_view_seconds * hz) as i64;
    ctx.board.worker_view_until = 0;
    ctx.board
        .cool(key, ctx.tick, config.worker_view_interval_seconds * hz);
}

pub fn survey_workers(ctx: &mut Context<'_>) {
    let config = ctx.config;
    let tick = ctx.tick as i64;
    if ctx.board.army_view_until != 0 {
        if tick >= ctx.board.army_view_until {
            ctx.focus_view(None);
            ctx.board.army_view_until = 0;
        }
        return;
    }
    if !ctx.late_economy && ctx.board.worker_view_until == 0 {
        return;
    }
    let threat = match ctx.board.threat_tile {
        Some(at) if ctx.board.threat_until > tick => {
            ctx.settlement_distance(at) <= f64::from(config.defend_radius)
        }
        _ => false,
    };
    if ctx.board.worker_view_until != 0 {
        if tick >= ctx.board.worker_view_until || threat {
            ctx.focus_view(None);
            ctx.board.worker_view_until = 0;
        }
        return;
    }
    if !ctx.late_economy || threat || ctx.board.cooling("worker-view", ctx.tick) {
        return;
    }
    if ctx.villagers.len() < config.worker_view_idle_minimum {
        return;
    }
    let side = config.worker_view_cell_tiles;
    let mut missing: BTreeMap<Tile, usize> = BTreeMap::new();
    for worker in &ctx.villagers {
        if !unemployed(worker)
            || ctx.busy.contains(&worker.id)
            || ctx.reserved_workers.contains(&worker.id)
        {
            continue;
        }
        if !ctx.world.land_known(worker.tile()) {
            *missing
                .entry((worker.x.div_euclid(side), worker.y.div_euclid(side)))
                .or_insert(0) += 1;
        }
    }
    let hz = ctx.hz();
    ctx.board.cool(
        "worker-view",
        ctx.tick,
        config.worker_view_interval_seconds * hz,
    );
    let mut cells: Vec<(Tile, usize)> = missing.into_iter().collect();
    cells.sort_by_key(|(cell, count)| (std::cmp::Reverse(*count), *cell));
    for ((x, y), count) in cells {
        if count < config.worker_view_idle_minimum {
            break;
        }
        let cell = format!("worker-view:{x}:{y}");
        if ctx.board.cooling(&cell, ctx.tick) {
            continue;
        }
        ctx.focus_view(Some((x * side + side / 2, y * side + side / 2)));
        ctx.board.worker_view_until = (ctx.tick + config.worker_view_seconds * hz) as i64;
        ctx.board
            .cool(&cell, ctx.tick, config.worker_view_revisit_seconds * hz);
        break;
    }
}
