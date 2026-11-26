//! The scout reveals the starting area, then rides the circle around the map
//! centre that passes through our own base, opposite side first: with villages
//! spread evenly over the map that is where the enemies are. What it uncovers
//! lands in the fog memory the SDK keeps; the combat manager targets from there
//! (`ai/aoe_ai/play/scouting.py`).
use std::f64::consts::TAU;

use aoe_sdk::{EntityId, Order, TileCoord, UnitTask, Visibility};

use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{clamp_tile, dist, ring_points};
use crate::view::Tile;

/// Pick a public destination and let the server plan the full route.
pub fn mission_destination(ctx: &Context<'_>, goal: Tile) -> Option<Tile> {
    for radius in [0, 2, 4, 8] {
        let offsets = if radius == 0 {
            vec![(0, 0)]
        } else {
            vec![
                (radius, 0),
                (-radius, 0),
                (0, radius),
                (0, -radius),
                (radius, radius),
                (-radius, radius),
                (radius, -radius),
                (-radius, -radius),
            ]
        };
        for (dx, dy) in offsets {
            let tile = clamp_tile((goal.0 + dx, goal.1 + dy), ctx.world.map_size(), 3);
            if ctx.board.bad_moves.contains(&tile) {
                continue;
            }
            if ctx.world.visibility(tile) == Visibility::Unexplored
                || ctx.world.passable(tile) != Some(false)
            {
                return Some(tile);
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScoutManager {
    /// Python's manager keeps nothing: its state is on the blackboard.
    stateless: std::marker::PhantomData<()>,
}

impl ScoutManager {
    pub fn step(&self, ctx: &mut Context<'_>) -> Result<(), OrderBudgetExhausted> {
        if ctx.home.is_none() {
            return Ok(());
        }
        let cfg = ctx.config;
        let reach = f64::from(cfg.scout_unreachable_tiles);
        let (stall, settle) = (
            cfg.march_stall_seconds * ctx.hz(),
            cfg.scout_settle_seconds * ctx.hz(),
        );
        let scouts = ctx.scouts.clone();
        let live: Vec<u32> = scouts.iter().map(|s| s.id).collect();
        let board = &mut *ctx.board;
        board.scout_sent.retain(|id, _| live.contains(id));
        board.scout_targets.retain(|id, _| live.contains(id));
        board.scout_progress.retain(|id, _| live.contains(id));
        for scout in &scouts {
            if ctx.busy.contains(&scout.id) || scout.attack_target.is_some() {
                continue;
            }
            if scout.moving || scout.task == UnitTask::Moving {
                let (mut tile, mut since) = ctx
                    .board
                    .scout_progress
                    .get(&scout.id)
                    .copied()
                    .unwrap_or((scout.tile(), ctx.tick));
                if dist(tile, scout.tile()) >= 2.0 {
                    (tile, since) = (scout.tile(), ctx.tick);
                }
                ctx.board.scout_progress.insert(scout.id, (tile, since));
                if ctx.tick.saturating_sub(since) >= stall {
                    ctx.order_or_stop(
                        "stop",
                        Order::Stop {
                            unit: EntityId(scout.id),
                        },
                    )?;
                    ctx.board.scout_progress.remove(&scout.id);
                }
                continue;
            }
            if !scout.idle {
                continue;
            }
            let target = ctx.board.scout_targets.get(&scout.id).copied();
            if let Some(target) = target
                && target != scout.tile()
                && let Some(sent) = ctx.board.scout_sent.get(&scout.id).copied()
                && ctx.tick.saturating_sub(sent) >= settle
            {
                // A frontier can be dry land but occupied by a tree. Do not
                // send the idle scout back to that same failed tile forever.
                if ctx.board.bad_moves.len() >= 4096 {
                    ctx.board.bad_moves.clear();
                }
                ctx.board.bad_moves.insert(target);
            }
            if let Some(target) = target
                && dist(scout.tile(), target) > reach
            {
                // The ride ended far short of its waypoint: there is no route
                // from here, and waypoints as far away would fail the same way.
                let board = &mut *ctx.board;
                let index = board.scout_index.min(board.scout_waypoints.len());
                let rest = board.scout_waypoints.split_off(index);
                board
                    .scout_waypoints
                    .extend(rest.into_iter().filter(|p| dist(*p, scout.tile()) <= reach));
            }
            if ctx.board.scout_index >= ctx.board.scout_waypoints.len() {
                self.extend(ctx);
                if ctx.board.scout_index >= ctx.board.scout_waypoints.len() {
                    return Ok(());
                }
            }
            let goal = ctx.board.scout_waypoints[ctx.board.scout_index];
            // A route that is an empty list counts as none, as in Python.
            let path = ctx.land_route(scout.tile(), goal).filter(|p| !p.is_empty());
            let mut target = if path.is_some() {
                Some(goal)
            } else {
                ctx.land_frontier(scout.tile(), goal)
            };
            if let Some(found) = target {
                let found = ctx.unrefused(found);
                target = ctx.land_route(scout.tile(), found).map(|_| found);
            }
            if target.is_none() && ctx.board.scout_stage >= 2 {
                target = mission_destination(ctx, goal);
            }
            let Some(target) = target.filter(|t| *t != scout.tile()) else {
                ctx.board.scout_index += 1;
                continue;
            };
            if target == goal {
                ctx.board.scout_index += 1;
            }
            let tick = ctx.tick;
            let board = &mut *ctx.board;
            board.scout_targets.insert(scout.id, target);
            board.scout_sent.insert(scout.id, tick);
            board.scout_progress.insert(scout.id, (scout.tile(), tick));
            let order = Order::Move {
                unit: EntityId(scout.id),
                to: TileCoord::new(target.0, target.1),
            };
            ctx.order_or_stop("move", order)?;
        }
        Ok(())
    }

    /// Queue the next leg: home ring, map circle, then ever wider rings.
    pub fn extend(&self, ctx: &mut Context<'_>) {
        let cfg = ctx.config;
        let size = ctx.world.map_size();
        let home = ctx.home.unwrap_or_default();
        let stage = ctx.board.scout_stage;
        ctx.board.scout_stage += 1;
        let ring = |radius: i32, spacing: i32| -> Vec<Tile> {
            ring_points(home, radius, spacing)
                .into_iter()
                .map(|p| clamp_tile(p, size, 3))
                .collect()
        };
        match stage {
            0 => ctx.board.scout_waypoints.extend(ring(18, 14)),
            1 => {
                let circle = self.map_circle(ctx);
                ctx.board.scout_waypoints.extend(circle);
            }
            _ => {
                let radius = cfg.scout_ring_step * (stage as i32 - 1);
                if radius > cfg.scout_max_radius {
                    ctx.board.scout_stage = 1; // start over: the fog may have changed
                    return;
                }
                ctx.board.scout_radius = radius;
                ctx.board.scout_waypoints.extend(ring(radius, 10));
            }
        }
    }

    /// Points on the circle round the map centre through home, far ones first.
    pub fn map_circle(&self, ctx: &Context<'_>) -> Vec<Tile> {
        let (width, height) = ctx.world.map_size();
        let centre = (f64::from(width) / 2.0, f64::from(height) / 2.0);
        let home = ctx.home.unwrap_or_default();
        let (dx, dy) = (f64::from(home.0) - centre.0, f64::from(home.1) - centre.1);
        let radius = dx.hypot(dy).max(f64::from(width.min(height)) / 4.0);
        let start = dy.atan2(dx);
        let count = ((2.0 * std::f64::consts::PI * radius / 40.0) as usize).max(8);
        let mut points: Vec<Tile> = (1..count)
            .map(|i| {
                let angle = start + TAU * i as f64 / count as f64;
                (
                    (centre.0 + radius * angle.cos()) as i32,
                    (centre.1 + radius * angle.sin()) as i32,
                )
            })
            .collect();
        // Walk the far half outward from the mirror point so the route stays short.
        points.sort_by(|a, b| dist(*b, home).total_cmp(&dist(*a, home)));
        points
            .into_iter()
            .map(|p| clamp_tile(p, (width, height), 3))
            .collect()
    }
}
