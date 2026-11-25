//! Threat-funded defences on public resource sites and known enemy approaches
//! (`ai/aoe_ai/play/fortification/__init__.py`).
pub mod threat;

use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{BuildingKind, EntityId, Order, TileCoord};

use self::threat::threatened;
use crate::blackboard::Plan;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{clamp_tile, dist, dist2};
use crate::play::constructor::builder_near;
use crate::roster::{building_price, building_size};
use crate::view::Tile;

type Done<T = ()> = Result<T, OrderBudgetExhausted>;

/// A threatened site and the reach within which a tower serves it.
type Site = (Tile, i32);

/// The enemy-facing edge of the nearest surviving settlement.
pub fn frontier(ctx: &Context<'_>) -> Option<Tile> {
    let enemies = ctx.world.known_enemy_buildings();
    let mut best: Option<(Tile, Tile, f64)> = None;
    for center in &ctx.centers {
        for enemy in &enemies {
            let d = dist(*center, enemy.center());
            if best.is_none_or(|(.., least)| d < least) {
                best = Some((*center, enemy.center(), d));
            }
        }
    }
    let (center, enemy, d) = best?;
    let length = d.max(1.0);
    let reach = ctx.config.fort_frontier_distance;
    let toward = |c: i32, e: i32| {
        (f64::from(c) + f64::from((e - c) * reach) / length).round_ties_even() as i32
    };
    Some(clamp_tile(
        (toward(center.0, enemy.0), toward(center.1, enemy.1)),
        ctx.world.map_size(),
        3,
    ))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FortificationManager {
    /// Whether the last `place` was refused by the engine or the site, as
    /// opposed to merely lacking budget.
    refused: bool,
}

impl FortificationManager {
    /// Prefer a narrow explored cross-section of a reachable approach.
    ///
    /// Unknown terrain is never counted as a wall. The local probe has a fixed
    /// size and the public land queries share the decision's existing budget.
    pub fn lane(
        &self,
        ctx: &mut Context<'_>,
        center: Tile,
        rejected: &BTreeSet<Tile>,
    ) -> Option<Tile> {
        let cfg = ctx.config;
        let enemies = ctx.world.known_enemy_buildings();
        let mut goal = None;
        let mut least = f64::INFINITY;
        for enemy in &enemies {
            let d = dist(center, enemy.center());
            if d < least {
                (goal, least) = (Some(enemy.center()), d);
            }
        }
        let goal = goal?;
        let length = least.max(1.0);
        let (dx, dy) = (goal.0 - center.0, goal.1 - center.1);
        let across = if dx.abs() > dy.abs() { (0, 1) } else { (1, 0) };
        let gate_width = building_size(BuildingKind::StoneGate).0;
        let size = ctx.world.map_size();
        let mut candidates: Vec<(i32, i32, Tile)> = Vec::new();
        for reach in cfg.fort_lane_distances {
            let along = |d: i32| f64::from(d * reach) / length;
            let point = clamp_tile(
                (
                    (f64::from(center.0) + along(dx)).round_ties_even() as i32,
                    (f64::from(center.1) + along(dy)).round_ties_even() as i32,
                ),
                size,
                3,
            );
            let wing = cfg.fort_wall_wing;
            let cross: Vec<Option<bool>> = (-wing..=wing)
                .map(|n| {
                    ctx.world
                        .passable((point.0 + across.0 * n, point.1 + across.1 * n))
                })
                .collect();
            if cross.iter().any(Option::is_none) || ctx.world.passable(point) != Some(true) {
                continue;
            }
            let width = cross.iter().filter(|tile| **tile == Some(true)).count() as i32;
            // The gate must fit the opening, and its own footprint must be known open.
            if width < gate_width
                || (0..gate_width)
                    .any(|n| ctx.world.passable((point.0 - 2 + n, point.1)) != Some(true))
            {
                continue;
            }
            if rejected.contains(&point) {
                continue;
            }
            let route = ctx.land_route(center, point);
            if route.is_none() && ctx.land_unanswered(center, point) {
                return None; // budget spent: choose on a later decision, never on partial answers
            }
            if route.is_some() {
                candidates.push((width, (reach - cfg.fort_lane_distances[1]).abs(), point));
            }
        }
        candidates.into_iter().min().map(|(_, _, point)| point)
    }

    pub fn step(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        if !cfg.fortifications
            || !ctx.castle()
            || ctx.home.is_none()
            || ctx.villagers.len() < cfg.fort_workers_per_tower as usize
            || ctx.board.cooling("fortify", ctx.tick)
        {
            return Ok(());
        }
        let wait = cfg.fort_interval_seconds * ctx.hz();
        ctx.board.cool("fortify", ctx.tick, wait);
        let mut towers: Vec<Tile> = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::WatchTower)
            .map(|b| b.center())
            .collect();
        towers.extend(
            ctx.board
                .plans
                .iter()
                .filter(|p| p.kind == BuildingKind::WatchTower)
                .map(|p| p.tile),
        );
        let frontiers: Vec<Tile> = ctx
            .centers
            .iter()
            .copied()
            .filter(|c| threatened(ctx, *c))
            .collect();
        // Resource drop-offs are paid evidence of economic activity, including
        // remote mines and forests. Threat is judged per site, so a camp far
        // from every Town Center can authorise its own tower. No speculative
        // towers in fog and none at quiet sites.
        let camps: Vec<Tile> = ctx
            .buildings
            .iter()
            .filter(|b| {
                b.complete
                    && matches!(
                        b.kind,
                        BuildingKind::MiningCamp | BuildingKind::LumberCamp | BuildingKind::Mill
                    )
                    && threatened(ctx, b.center())
            })
            .map(|b| b.center())
            .collect();
        if frontiers.is_empty() && camps.is_empty() {
            return Ok(());
        }
        let camp_reach = cfg.fort_tower_coverage + cfg.fort_site_radius;
        let center_reach =
            cfg.fort_lane_distances.into_iter().max().unwrap_or(0) + cfg.fort_lane_tower_radius;
        let mut sites: Vec<Site> = frontiers.iter().map(|c| (*c, center_reach)).collect();
        sites.extend(camps.iter().map(|c| (*c, camp_reach)));
        sites.sort();
        // Every tower belongs to exactly one site: the nearest threatened site
        // whose reach contains it, ties broken by the site's position. Overlapping
        // reaches can then neither starve a site nor let it exceed its cap.
        let owner_of = |point: Tile| -> Option<Site> {
            let mut owner: Option<(f64, Site)> = None;
            for site in &sites {
                let d = dist(site.0, point);
                if d > f64::from(site.1) {
                    continue;
                }
                let better = owner.is_none_or(|(least, held)| (d, *site) < (least, held));
                if better {
                    owner = Some((d, *site));
                }
            }
            owner.map(|(_, site)| site)
        };
        let mut held: BTreeMap<Site, usize> = sites.iter().map(|site| (*site, 0)).collect();
        for tower in &towers {
            if let Some(owner) = owner_of(*tower) {
                *held.entry(owner).or_insert(0) += 1;
            }
        }
        let active: usize = held.values().sum();
        let ceiling = ctx.villagers.len() / cfg.fort_workers_per_tower as usize;
        let mut targets: Vec<(Tile, i32)> =
            camps.iter().map(|c| (*c, cfg.fort_site_radius)).collect();
        let mut lane = None;
        let mut center = None;
        if !frontiers.is_empty() {
            let chosen = frontiers[ctx.board.fort_cursor % frontiers.len()];
            center = Some(chosen);
            ctx.board.fort_cursor += 1;
            // Retain a layout while it is being built; new scouting must not move
            // the gate between its two wings. Forget lost settlements.
            let centers = &ctx.centers;
            ctx.board.fort_lanes.retain(|c, _| centers.contains(c));
            ctx.board.fort_rejected.retain(|c, _| centers.contains(c));
            lane = ctx.board.fort_lanes.get(&chosen).copied();
            if lane.is_none() {
                let rejected = ctx
                    .board
                    .fort_rejected
                    .get(&chosen)
                    .cloned()
                    .unwrap_or_default();
                lane = self.lane(ctx, chosen, &rejected);
                if let Some(found) = lane {
                    ctx.board.fort_lanes.insert(chosen, found);
                }
            }
            if let Some(lane) = lane {
                let axis_x = (lane.0 - chosen.0).abs() > (lane.1 - chosen.1).abs();
                let mut tower = lane;
                let (value, origin) = if axis_x {
                    (&mut tower.0, chosen.0)
                } else {
                    (&mut tower.1, chosen.1)
                };
                let sign = if *value > origin { 1 } else { -1 };
                *value -= cfg.fort_lane_tower_offset * sign;
                targets.insert(0, (tower, cfg.fort_lane_tower_radius));
            }
        }
        if active < ceiling {
            for (target, radius) in targets {
                let covered = towers
                    .iter()
                    .any(|tower| dist(target, *tower) <= f64::from(cfg.fort_tower_coverage));
                if covered {
                    continue;
                }
                // Cap the site the nearest-site rule will credit once it stands.
                if let Some(owner) = owner_of(target)
                    && held[&owner] >= cfg.fort_towers_per_frontier as usize
                {
                    continue;
                }
                if self.place(ctx, BuildingKind::WatchTower, target, false, Some(radius))? {
                    return Ok(());
                }
            }
        }
        if let (Some(lane), Some(center)) = (lane, center)
            && ctx.villagers.len() >= cfg.fort_wall_workers as usize
        {
            self.wall(ctx, center, lane)?;
        }
        Ok(())
    }

    fn place(
        &mut self,
        ctx: &mut Context<'_>,
        kind: BuildingKind,
        target: Tile,
        exact: bool,
        radius: Option<i32>,
    ) -> Done<bool> {
        let cfg = ctx.config;
        self.refused = false;
        if ctx.board.cooling("world-limit:build", ctx.tick) {
            return Ok(false);
        }
        let cost = building_price(kind);
        // Leave an essential wood float and the next Town Center's stone.
        let floated = [
            cost[0],
            cost[1] + cfg.fort_wood_float,
            cost[2],
            cost[3] + cfg.fort_stone_float,
        ];
        if !ctx.can_afford(floated) {
            return Ok(false);
        }
        let Some(builder) = builder_near(ctx, target) else {
            return Ok(false);
        };
        let queries = ctx.site_queries();
        let reach = if exact {
            1
        } else {
            radius.filter(|r| *r != 0).unwrap_or(cfg.fort_site_radius)
        };
        let site = ctx.find_build_site(kind, target, reach, 0);
        let bad = site.is_some_and(|s| {
            ctx.board
                .bad_sites
                .contains(&(format!("{kind:?}"), s.0, s.1))
        });
        let Some(site) = site.filter(|s| !(exact && *s != target) && !bad) else {
            // An exhausted query budget says nothing about the site itself.
            self.refused = ctx.site_queries() > queries || bad;
            return Ok(false);
        };
        let (width, height) = building_size(kind);
        let (x, y) = site;
        let from = builder.tile();
        let mut edges = [(x - 1, y), (x + width, y), (x, y - 1), (x, y + height)];
        edges.sort_by_key(|tile| dist2(from, *tile));
        let mut reachable = false;
        for edge in edges {
            if ctx.land_route(from, edge).is_some() {
                reachable = true;
                break;
            }
            if ctx.land_unanswered(from, edge) {
                return Ok(false); // no answer yet: not a refusal
            }
        }
        if !reachable {
            self.refused = true;
            return Ok(false);
        }
        ctx.spend(cost)?;
        let order = Order::Build {
            builders: vec![EntityId(builder.id)],
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
        ctx.board
            .builders
            .insert(builder.id, (format!("{kind:?}"), site));
        ctx.board.bump("fortifications_placed");
        Ok(true)
    }

    /// One exact footprint at a time, gate first, preserving worker access.
    ///
    /// The ordinary Build command makes a horizontal four-tile gate. For an
    /// east/west threat its wings turn north/south at opposite gate ends,
    /// making a stepped line across the approach without a closed opening.
    fn wall(&mut self, ctx: &mut Context<'_>, center: Tile, lane: Tile) -> Done {
        let (x, y) = lane;
        let origin = (x - 2, y);
        let gate = ctx
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::StoneGate && (b.x, b.y) == origin)
            .map(|b| b.complete);
        let planned = |ctx: &Context<'_>, kind: BuildingKind, tile: Tile| {
            ctx.board
                .plans
                .iter()
                .any(|p| p.kind == kind && p.tile == tile)
        };
        let Some(complete) = gate else {
            if !planned(ctx, BuildingKind::StoneGate, origin) {
                let placed = self.place(ctx, BuildingKind::StoneGate, origin, true, None)?;
                if !placed && self.refused {
                    // Unbuildable here: forget this lane so the next sampled one is tried.
                    ctx.board.fort_lanes.remove(&center);
                    ctx.board
                        .fort_rejected
                        .entry(center)
                        .or_default()
                        .insert(lane);
                }
            }
            return Ok(());
        };
        if !complete {
            return Ok(());
        }
        let vertical = (lane.0 - center.0).abs() > (lane.1 - center.1).abs();
        let length = ctx.config.fort_wall_wing;
        let mut tiles: Vec<Tile> = Vec::new();
        if vertical {
            tiles.extend((1..=length).map(|i| (x - 2, y - i)));
            tiles.extend((1..=length).map(|i| (x + 1, y + i)));
        } else {
            tiles.extend((1..=length).map(|i| (x - 2 - i, y)));
            tiles.extend((1..=length).map(|i| (x + 1 + i, y)));
        }
        let occupied: BTreeSet<Tile> = ctx
            .buildings
            .iter()
            .filter(|b| matches!(b.kind, BuildingKind::StoneWall | BuildingKind::StoneGate))
            .map(|b| (b.x, b.y))
            .collect();
        for tile in tiles {
            if occupied.contains(&tile) || planned(ctx, BuildingKind::StoneWall, tile) {
                continue;
            }
            if self.place(ctx, BuildingKind::StoneWall, tile, true, None)? {
                return Ok(());
            }
        }
        Ok(())
    }
}
