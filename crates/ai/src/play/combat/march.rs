//! Bounded formation routing shared by the field army and staging orders
//! (`ai/aoe_ai/play/combat/march.py`).
use std::collections::BTreeMap;

use aoe_sdk::{EntityId, Order, TileCoord};

use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{centroid, dist};
use crate::play::scouting::mission_destination;
use crate::view::{Tile, Unit};
use crate::worker_view::focus_army;

/// A group of soldiers: its map cell, and the soldier's id when it marches alone.
type Cell = (i64, i64, i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Attack,
    Recall,
    Rally,
}

impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Purpose::Attack => "attack",
            Purpose::Recall => "recall",
            Purpose::Rally => "rally",
        }
    }
}

pub fn march(
    ctx: &mut Context<'_>,
    army: &[Unit],
    middle: Tile,
    goal: Tile,
    purpose: Purpose,
) -> Result<(), OrderBudgetExhausted> {
    let config = ctx.config;
    let pause = (config.march_pause_seconds * ctx.hz()) as i64;
    let side = config.march_group_tiles;
    let recalling = purpose == Purpose::Recall;
    let tick = ctx.tick as i64;
    let mut cells: BTreeMap<Cell, Vec<Unit>> = BTreeMap::new();
    for soldier in army {
        let last = ctx
            .board
            .marched
            .get(&soldier.id)
            .map_or(-pause, |t| *t as i64);
        // A soldier that is fighting is never re-ordered: a recall or a hop
        // would walk it out of the fight it is winning.
        if ctx.busy.contains(&soldier.id) || soldier.attack_target.is_some() {
            continue;
        }
        let replace = recalling && last < ctx.board.raid_started;
        if !(replace || (soldier.idle && tick - last >= pause)) {
            continue;
        }
        let independent = purpose == Purpose::Attack
            && ctx
                .board
                .cooling(&format!("march-alone:{}", soldier.id), ctx.tick);
        let key = (
            i64::from(soldier.x.div_euclid(side)),
            i64::from(soldier.y.div_euclid(side)),
            if independent {
                i64::from(soldier.id)
            } else {
                -1
            },
        );
        cells.entry(key).or_default().push(soldier.clone());
    }

    let keys: Vec<Cell> = cells.keys().copied().collect();
    // `bisect_right`: the first cell past the one the last march stopped at.
    let start = match ctx.board.march_after.get(purpose.name()) {
        Some(after) => keys.partition_point(|key| key <= after),
        None => 0,
    };
    for cell in keys[start..].iter().chain(&keys[..start]) {
        if ctx.orders >= ctx.order_limit
            || (purpose != Purpose::Attack && ctx.land_budget_exhausted())
        {
            return Ok(());
        }
        ctx.board
            .march_after
            .insert(purpose.name().to_owned(), *cell);
        let mut soldiers = cells.remove(cell).unwrap_or_default();
        let tiles: Vec<Tile> = soldiers.iter().map(Unit::tile).collect();
        let mut origin = centroid(&tiles).unwrap_or(middle);
        if !ctx.world.land_known(origin) {
            origin = soldiers
                .iter()
                .map(Unit::tile)
                .find(|t| ctx.world.land_known(*t))
                .unwrap_or(origin);
        }
        let mut target = ctx.land_hop(origin, goal);
        if target == origin
            && purpose == Purpose::Attack
            && (!ctx.world.land_known(origin) || !ctx.world.land_known(goal))
        {
            target = mission_destination(ctx, goal).unwrap_or(origin);
        }
        if target == origin
            && purpose != Purpose::Rally
            && let Some(reachable) = ctx.land_frontier(origin, goal)
        {
            target = ctx.land_hop(origin, reachable);
        }
        if target == origin && recalling {
            // A player can order a short move toward their own raided base
            // when their terrain view is elsewhere. The server checks the hop.
            let reach = (f64::from(config.march_hop_tiles) / dist(origin, goal).max(1.0)).min(1.0);
            let step =
                |a: i32, b: i32| (f64::from(a) + f64::from(b - a) * reach).round_ties_even() as i32;
            target = ctx.unrefused((step(origin.0, goal.0), step(origin.1, goal.1)));
        }
        if target == origin {
            if purpose != Purpose::Rally && !ctx.world.land_known(origin) {
                focus_army(ctx, origin);
            }
            continue;
        }
        let marched = &ctx.board.marched;
        soldiers.sort_by_key(|u| (marched.get(&u.id).map_or(-pause, |t| *t as i64), u.id));
        for members in soldiers.chunks(config.march_group_members) {
            if ctx.orders >= ctx.order_limit {
                return Ok(());
            }
            let order = Order::FormationMove {
                units: members.iter().map(|u| EntityId(u.id)).collect(),
                formation: config.army_formation,
                to: TileCoord::new(target.0, target.1),
                queued: false,
                attack_move: purpose == Purpose::Attack,
            };
            ctx.order_or_stop("formation_move", order)?;
            for unit in members {
                ctx.board.marched.insert(unit.id, ctx.tick);
                ctx.board
                    .march_progress
                    .insert(unit.id, (unit.tile(), ctx.tick));
            }
            if recalling {
                *ctx.board
                    .stats
                    .entry("defenders_recalled".to_owned())
                    .or_insert(0) += members.len() as u64;
            }
        }
    }
    Ok(())
}
