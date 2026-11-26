//! New Town Centers and Castles, and the placing both need.
use std::f64::consts::PI;

use aoe_sdk::{BuildingKind, EntityId, Order, TileCoord, UnitTask};

use super::{Done, ManagementManager};
use crate::blackboard::Plan;
use crate::context::{Context, OrderBudgetExhausted};
use crate::geometry::{clamp_tile, dist, dist2};
use crate::play::buildorder;
use crate::play::fortification::frontier;
use crate::roster::{building_price, building_size};
use crate::view::{Tile, Unit};

impl ManagementManager {
    /// A second (third...) Town Center on the way to the map centre.
    pub fn expand(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        let total = buildorder::planned_count(ctx, BuildingKind::TownCenter);
        if total >= buildorder::wanted_centers(ctx)
            || (total > 0 && (ctx.villagers.len() < cfg.expand_villagers as usize || !ctx.castle()))
            || ctx.board.cooling("expand", ctx.tick)
        {
            return Ok(());
        }
        let cost = building_price(BuildingKind::TownCenter);
        if !ctx.can_afford(cost) {
            // Castle Age is bought: keep the wood back for the Town Center, shares lean to wood.
            let wood = ctx.economy.as_ref().map_or(0, |e| i64::from(e.wood));
            let spare = wood - i64::from(ctx.spent[1]) - i64::from(cfg.expansion_wood_float);
            ctx.spent[1] += i64::from(cost[1]).min(spare).max(0) as u32;
            if total == 0 || ctx.army.len() + ctx.led.len() >= cfg.imperial_army_min as usize {
                ctx.saving_castle = true;
            }
            ctx.board.bump("saving_for_town_center");
            return Ok(());
        }
        let centers: Vec<Tile> = ctx
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::TownCenter)
            .map(|b| b.center())
            .collect();
        if centers.is_empty() && ctx.villagers.is_empty() {
            return Ok(()); // nobody left to build with; the rest of the decision continues
        }
        let origin = if centers.is_empty() {
            ctx.villagers[0].tile()
        } else {
            centers[ctx.board.expansion_cursor % centers.len()]
        };
        ctx.board.expansion_cursor += 1;
        let (width, height) = ctx.world.map_size();
        let toward = f64::from(height / 2 - origin.1).atan2(f64::from(width / 2 - origin.0));
        let spacing = f64::from(cfg.settlement_spacing);
        for turn in 0..6 {
            let step = (turn + ctx.board.expansion_cursor % 6) as f64;
            let angle = toward + step * PI / 3.0;
            let reach = angle.cos() * spacing * 1.6;
            let side = angle.sin() * spacing * 1.6;
            let mut near = clamp_tile(
                (
                    (f64::from(origin.0) + reach).round_ties_even() as i32,
                    (f64::from(origin.1) + side).round_ties_even() as i32,
                ),
                (width, height),
                3,
            );
            if total == 0 {
                near = origin;
            }
            if centers
                .iter()
                .any(|c| dist(near, *c) < f64::from(cfg.settlement_spacing))
            {
                continue;
            }
            if ctx.land_route(origin, near).is_none() {
                continue;
            }
            let builders = self.builders(ctx, near, 4);
            if !builders.is_empty()
                && self.place(
                    ctx,
                    BuildingKind::TownCenter,
                    near,
                    &builders,
                    (8, 1),
                    false,
                )?
            {
                ctx.board.bump("expansions");
                let wait = 40 * ctx.hz();
                ctx.board.cool("expand", ctx.tick, wait);
                return Ok(());
            }
        }
        let wait = 20 * ctx.hz();
        ctx.board.cool("expand", ctx.tick, wait);
        Ok(())
    }

    pub fn castles(&mut self, ctx: &mut Context<'_>) -> Done {
        let cfg = ctx.config;
        if cfg.castles == 0 || !ctx.castle() || ctx.board.cooling("castle", ctx.tick) {
            return Ok(());
        }
        let total = buildorder::planned_count(ctx, BuildingKind::Castle);
        if total >= buildorder::wanted_castles(ctx) {
            return Ok(());
        }
        if !ctx.can_afford(building_price(BuildingKind::Castle)) {
            return Ok(());
        }
        let near = match ctx.centers.len() {
            0 => match ctx.home {
                Some(home) => home,
                None => return Ok(()),
            },
            len => ctx.centers[total % len],
        };
        // The frontier may lie across water from every builder: require a land-reachable
        // footprint there, else fall back to the settlement itself.
        let mut tiles = Vec::new();
        for tile in [frontier(ctx), Some(near)].into_iter().flatten() {
            if !tiles.contains(&tile) {
                tiles.push(tile);
            }
        }
        for tile in tiles {
            let builders = self.builders(ctx, tile, 5);
            self.unanswered = false;
            if let Some(first) = builders.first()
                && ctx.land_route(first.tile(), tile).is_none()
            {
                if ctx.land_unanswered(first.tile(), tile) {
                    break; // budget spent: retry later instead of treating it as unreachable
                }
                continue; // across water or walls: spend no site queries here
            }
            if !builders.is_empty()
                && self.place(ctx, BuildingKind::Castle, tile, &builders, (24, 2), true)?
            {
                ctx.board.bump("castles_placed");
                break;
            }
            if self.unanswered {
                break;
            }
        }
        let wait = 10 * ctx.hz();
        ctx.board.cool("castle", ctx.tick, wait);
        Ok(())
    }

    /// The `count` free villagers nearest to `tile`.
    pub(super) fn builders(&self, ctx: &Context<'_>, tile: Tile, count: usize) -> Vec<Unit> {
        let farmers: Vec<u32> = ctx.board.farmers.values().copied().collect();
        let mut free: Vec<&Unit> = ctx
            .villagers
            .iter()
            .filter(|v| {
                v.alive
                    && !ctx.busy.contains(&v.id)
                    && !matches!(
                        v.task,
                        UnitTask::Building | UnitTask::Seeding | UnitTask::Farming
                    )
                    && !farmers.contains(&v.id)
                    && !ctx.reserved_workers.contains(&v.id)
            })
            .collect();
        free.sort_by_key(|v| dist2(v.tile(), tile));
        free.into_iter().take(count).cloned().collect()
    }

    pub(super) fn place(
        &mut self,
        ctx: &mut Context<'_>,
        kind: BuildingKind,
        near: Tile,
        builders: &[Unit],
        (radius, gap): (i32, i32),
        reachable: bool,
    ) -> Result<bool, OrderBudgetExhausted> {
        let cost = building_price(kind);
        if ctx.board.cooling("world-limit:build", ctx.tick) || !ctx.try_spend(cost)? {
            return Ok(false);
        }
        let mut site = None;
        for (reach, spacing) in [
            (radius, gap),
            (radius, 0),
            (2 * radius, gap),
            (2 * radius, 0),
        ] {
            let Some(found) = ctx.find_build_site(kind, near, reach, spacing) else {
                continue;
            };
            let bad = ctx
                .board
                .bad_sites
                .contains(&(format!("{kind:?}"), found.0, found.1));
            let crowded = kind == BuildingKind::TownCenter
                && ctx.buildings.iter().any(|b| {
                    b.kind == kind
                        && dist(found, b.center()) < f64::from(ctx.config.settlement_spacing)
                });
            if bad || crowded {
                continue;
            }
            if reachable {
                match Self::landlocked(ctx, kind, found, builders) {
                    None => {
                        self.unanswered = true;
                        break;
                    }
                    Some(false) => continue,
                    Some(true) => {}
                }
            }
            site = Some(found);
            break;
        }
        let Some(site) = site else {
            for (spent, price) in ctx.spent.iter_mut().zip(cost) {
                *spent -= price;
            }
            return Ok(false);
        };
        let order = Order::Build {
            builders: builders.iter().map(|b| EntityId(b.id)).collect(),
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
        for b in builders {
            ctx.board.builders.insert(b.id, (format!("{kind:?}"), site));
        }
        Ok(true)
    }

    /// True when the first builder can walk to some edge of the footprint.
    ///
    /// None when the shared land-query budget left an edge unanswered.
    fn landlocked(
        ctx: &mut Context<'_>,
        kind: BuildingKind,
        site: Tile,
        builders: &[Unit],
    ) -> Option<bool> {
        let from = builders[0].tile();
        let (width, height) = building_size(kind);
        let (x, y) = site;
        let mut edges = [(x - 1, y), (x + width, y), (x, y - 1), (x, y + height)];
        edges.sort_by_key(|tile| dist2(from, *tile));
        for edge in edges {
            if ctx.land_route(from, edge).is_some() {
                return Some(true);
            }
            if ctx.land_unanswered(from, edge) {
                return None;
            }
        }
        Some(false)
    }
}
