//! The offensive half of the combat manager: choosing a building, marching on
//! it, and searching the map when none is known.
use std::f64::consts::PI;

use aoe_sdk::{BuildingKind, EntityId, Order};

use super::CombatManager;
use super::march::{Purpose, march};
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{centroid, clamp_tile, dist, nearest};
use crate::view::{Building, Tile, Unit};

impl CombatManager {
    pub fn target_building(
        &self,
        ctx: &mut Context<'_>,
        origin: Tile,
        unit: Option<u32>,
    ) -> Option<Building> {
        let countering =
            ctx.board.counter_owner.is_some() && ctx.board.counter_until > ctx.tick as i64;
        let mut known: Vec<Building> = ctx
            .world
            .known_enemy_buildings()
            .into_iter()
            .filter(|b| {
                !ctx.board.cooling(&format!("target:{}", b.id), ctx.tick)
                    && unit.is_none_or(|u| self.available_target(ctx, u, b.id))
            })
            .collect();
        if countering {
            let owner = ctx.board.counter_owner;
            let key = |b: &Building| {
                let base = matches!(b.kind, BuildingKind::TownCenter | BuildingKind::Castle);
                (!base, dist(origin, b.center()), b.id)
            };
            return known
                .into_iter()
                .filter(|b| Some(b.owner) == owner)
                .min_by(|a, b| {
                    let (ka, kb) = (key(a), key(b));
                    ka.0.cmp(&kb.0)
                        .then(ka.1.total_cmp(&kb.1))
                        .then(ka.2.cmp(&kb.2))
                });
        }
        let current = ctx.board.attack_target;
        known.sort_by(|a, b| {
            (Some(a.id) != current)
                .cmp(&(Some(b.id) != current))
                .then(dist(origin, a.center()).total_cmp(&dist(origin, b.center())))
                .then(a.id.cmp(&b.id))
        });
        if known.first().is_some_and(|b| Some(b.id) == current) {
            return known.into_iter().next();
        }
        let shortlist = ((ctx.config.land_queries_per_decision / 4) as usize).max(1);
        let mut reachable = None;
        for building in known.iter().take(shortlist) {
            if ctx.land_route(origin, building.center()).is_some() {
                reachable = Some(building.clone());
                break;
            }
        }
        reachable.or_else(|| {
            known
                .into_iter()
                .find(|b| !ctx.world.land_known(origin) || !ctx.world.land_known(b.center()))
        })
    }

    pub fn attack(&self, ctx: &mut Context<'_>, army: &[Unit]) -> Result<(), OrderBudgetExhausted> {
        let cfg = ctx.config;
        if army.is_empty() {
            let mem = &mut *ctx.board;
            mem.attacking = false;
            mem.attack_members.clear();
            return Ok(());
        }
        if (army.len() as f64) < cfg.retreat_fraction * f64::from(ctx.board.attack_launch_size) {
            let mem = &mut *ctx.board;
            mem.attacking = false;
            mem.attack_members.clear();
            mem.bump("retreats");
            return Ok(());
        }
        let tiles: Vec<Tile> = army.iter().map(Unit::tile).collect();
        let anchor = centroid(&tiles).or(ctx.home).unwrap_or_default();
        let middle = nearest(army, anchor, Unit::tile).map_or(anchor, Unit::tile);
        let Some(building) = self.target_building(ctx, middle, None) else {
            // The server forgets a destroyed building when its footprint is
            // seen. Distance from its centre cannot establish sight of a
            // remembered target.
            return self.search(ctx, army, middle);
        };
        ctx.board.attack_target = Some(building.id);
        if self.stalled(ctx, middle, building.center()) {
            let wait = cfg.march_stall_seconds * ctx.hz();
            let mem = &mut *ctx.board;
            mem.cool(&format!("target:{}", building.id), ctx.tick, wait);
            mem.attack_target = None;
            mem.march_goal = None;
            mem.bump("targets_unreachable");
            return Ok(());
        }
        // Goals in the order the first soldier of each was assigned.
        let mut marching: Vec<(Tile, Vec<Unit>)> = Vec::new();
        for unit in army {
            if !unit.idle || ctx.busy.contains(&unit.id) {
                continue;
            }
            // Shared-origin queries are cached; each march group checks its hop.
            // Per-soldier origins would exhaust the public query budget here.
            let mut target = building.clone();
            if !self.available_target(ctx, unit.id, building.id) {
                match self.target_building(ctx, middle, Some(unit.id)) {
                    Some(other) => target = other,
                    None => continue,
                }
            }
            if target.visible
                && dist(unit.tile(), target.center()) <= f64::from(cfg.march_hop_tiles)
            {
                let order = Order::Attack {
                    unit: EntityId(unit.id),
                    target: EntityId(target.id),
                };
                ctx.order_or_stop("attack", order)?;
            } else {
                let goal = if target.visible {
                    target.center()
                } else {
                    self.approach(ctx, &target, middle)
                };
                match marching.iter_mut().find(|(g, _)| *g == goal) {
                    Some((_, members)) => members.push(unit.clone()),
                    None => marching.push((goal, vec![unit.clone()])),
                }
            }
        }
        for (goal, members) in &marching {
            march(ctx, members, middle, *goal, Purpose::Attack)?;
        }
        Ok(())
    }

    /// A tile just outside the building toward `origin`: a move onto the
    /// building's own footprint is refused, but standing in sight starts the
    /// attack.
    pub fn approach(&self, ctx: &Context<'_>, building: &Building, origin: Tile) -> Tile {
        let (cx, cy) = building.center();
        let reach = f64::from(building.width.max(building.height)) / 2.0 + 3.0;
        let d = dist((cx, cy), origin).max(1.0);
        let bad = &ctx.board.bad_moves;
        let mut first = None;
        // A tile the server refused (a tree, a wall, water) is walked around.
        for extra in [0.0, 2.0, 4.0, 6.0] {
            for turn in [0.0_f64, 0.6, -0.6, 1.2, -1.2] {
                let ux = f64::from(origin.0 - cx) / d;
                let uy = f64::from(origin.1 - cy) / d;
                let (sin, cos) = turn.sin_cos();
                let point = clamp_tile(
                    (
                        (f64::from(cx) + (ux * cos - uy * sin) * (reach + extra)).round_ties_even()
                            as i32,
                        (f64::from(cy) + (ux * sin + uy * cos) * (reach + extra)).round_ties_even()
                            as i32,
                    ),
                    ctx.world.map_size(),
                    3,
                );
                first.get_or_insert(point);
                if !bad.contains(&point) {
                    return point;
                }
            }
        }
        first.unwrap_or_else(|| building.center())
    }

    /// No enemy building known: march across the map, the far side first, then
    /// round the centre, moving on whenever the army has arrived and found nothing.
    pub fn search(
        &self,
        ctx: &mut Context<'_>,
        army: &[Unit],
        middle: Tile,
    ) -> Result<(), OrderBudgetExhausted> {
        let size = ctx.world.map_size();
        let (width, height) = size;
        let home = ctx.home.unwrap_or_default();
        let far = (width - 1 - home.0, height - 1 - home.1);
        let mut ring: Vec<Tile> = (0..8)
            .map(|a| {
                let angle = f64::from(a) * PI / 4.0;
                (
                    (f64::from(width) / 2.0 + 0.33 * f64::from(width) * angle.cos()) as i32,
                    (f64::from(height) / 2.0 + 0.33 * f64::from(height) * angle.sin()) as i32,
                )
            })
            .collect();
        ring.sort_by(|a, b| dist(*b, home).total_cmp(&dist(*a, home)));
        let mut points = vec![far];
        points.extend(ring);
        if let Some(bearing) = ctx.board.counter_bearing
            && (ctx.tick as i64) < ctx.board.counter_until
        {
            let (bx, by) = (f64::from(bearing.0), f64::from(bearing.1));
            let scale = f64::from(width.min(height)) / bx.hypot(by).max(1.0);
            let aim = (
                (f64::from(home.0) + bx * scale).round_ties_even() as i32,
                (f64::from(home.1) + by * scale).round_ties_even() as i32,
            );
            points.insert(0, clamp_tile(aim, size, 3));
        }
        let leg = |index: usize| clamp_tile(points[index % points.len()], size, 3);
        let mut target = ctx.unrefused(leg(ctx.board.search_index));
        if dist(middle, target) <= 10.0 {
            ctx.board.search_index += 1;
            ctx.board.bump("search_legs");
            target = ctx.unrefused(leg(ctx.board.search_index));
        }
        if self.stalled(ctx, middle, target) {
            ctx.board.search_index += 1;
            ctx.board.bump("search_unreachable");
            return Ok(());
        }
        if !army.iter().any(|u| u.idle) {
            return Ok(());
        }
        march(ctx, army, middle, target, Purpose::Attack)
    }

    /// Track motion along detours, including legs away from the goal.
    pub fn stalled(&self, ctx: &mut Context<'_>, middle: Tile, goal: Tile) -> bool {
        let stall = ctx.config.march_stall_seconds * ctx.hz();
        let tick = ctx.tick;
        let mem = &mut *ctx.board;
        let moved = mem
            .march_anchor
            .is_none_or(|anchor| dist(middle, anchor) >= 8.0);
        if mem.march_goal != Some(goal) || moved {
            mem.march_goal = Some(goal);
            mem.march_anchor = Some(middle);
            mem.march_since = tick;
            return false;
        }
        tick.saturating_sub(mem.march_since) > stall
    }
}
