//! Reuse every paid-for plot, then tile farms around nearby food drop-offs.
use super::{
    VASSAL_RULES, VassalWork,
    construction::{FARM_DROPOFF_DISTANCE_SQUARED, FarmRepairSite},
    rules::Budget,
    snapshot::Snapshot,
};
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, TileCoord, TileRect};
use aoe_map::ResourceKind;
use std::collections::BTreeSet;

impl GameWorld {
    /// Adopted farms need the same short delivery trips as newly tiled farms.
    /// Prefer an unloaded idle helper, preserving productive farmers. If no
    /// helper is available, the farmer builds and then reclaims its own plot.
    pub(super) fn vassal_refresh_farms(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        // Each managed worker contributes at most one Farm. Retain a reseeding
        // builder's association too, so its search survives the rebuild.
        let associations: Vec<_> = self.economy.vassal_homes[&s.vassal]
            .workers
            .iter()
            .filter_map(|worker| {
                let (farm, gathering) = match self.economy.gatherers.get(worker) {
                    Some(task) => (task.farm?, true),
                    None => (self.economy.builders.get(worker)?.building, false),
                };
                let building = self.building(farm)?;
                if building.kind != BuildingKind::Farm
                    || self.vassal_controller(*worker) != Some(s.vassal)
                    || !s.home.covers(building.footprint, 0)
                {
                    return None;
                }
                Some((
                    farm,
                    *worker,
                    TileCoord::new(building.footprint.min.x + 1, building.footprint.min.y + 1),
                    gathering
                        && building.is_complete()
                        && self.unit(*worker).is_some_and(|u| u.carried.is_none()),
                ))
            })
            .collect();
        let dropoffs: Vec<_> = if associations.is_empty() {
            Vec::new()
        } else {
            s.buildings
                .iter()
                .filter(|b| {
                    b.is_complete()
                        && matches!(b.kind, BuildingKind::Mill | BuildingKind::TownCenter)
                })
                .map(|b| b.footprint)
                .collect()
        };
        let mut retained = BTreeSet::new();
        let mut targets = Vec::new();
        for (farm, worker, near, can_repair) in associations {
            if dropoffs.iter().any(|&rect| {
                super::super::dropoff::ring_distance(rect, near) <= FARM_DROPOFF_DISTANCE_SQUARED
            }) {
                continue;
            }
            retained.insert(farm);
            if can_repair {
                targets.push((farm, worker, near));
            }
        }
        s.home
            .repair_sites
            .retain(|farm, _| retained.contains(farm));
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.repair_sites.retain(|farm, _| retained.contains(farm));
        }
        let idle_builders: Vec<_> = if targets.is_empty() {
            Vec::new()
        } else {
            s.units
                .iter()
                .filter_map(|u| {
                    let live = self.unit(u.id)?;
                    (live.carried.is_none() && self.vassal_worker_idle(u.id, s.vassal))
                        .then_some((u.id, live.position.tile_floor()))
                })
                .collect()
        };
        // The entire repair phase gets at most half the remaining placement
        // work. Idle reuse and new foundations keep their share even when
        // several far crops have no legal Mill site.
        let site_limit = budget.sites
            + VASSAL_RULES
                .site_checks_per_domain
                .saturating_sub(budget.sites)
                / 2;
        targets.sort_unstable_by_key(|&(farm, _, _)| farm);
        let split = s.home.repair_after.map_or(0, |after| {
            targets.partition_point(|&(farm, _, _)| farm <= after)
        });
        for &(farm, worker, near) in targets[split..].iter().chain(&targets[..split]) {
            if !s.complete
                || budget.sites >= site_limit
                || budget.orders >= VASSAL_RULES.orders_per_domain
            {
                break;
            }
            s.home.repair_after = Some(farm);
            if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                home.repair_after = Some(farm);
            }
            let builder = idle_builders
                .iter()
                .min_by_key(|&&(id, at)| (super::super::placement::distance_squared(at, near), id))
                .map_or(worker, |&(id, _)| id);
            if self.vassal_mill_for_farm(
                builder,
                FarmRepairSite {
                    farm,
                    near,
                    site_limit,
                },
                s,
                budget,
                work,
            ) {
                self.claim_vassal_worker(s.vassal, builder, ResourceKind::Food);
                changed.push(builder);
                break;
            }
        }
    }

    pub(super) fn vassal_reclaim_farm(
        &mut self,
        worker: EntityId,
        s: &Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        let Some(unit) = self.unit(worker) else {
            return false;
        };
        let from = unit.position.tile_floor();
        // A completed farm's builder may still be retrying its work spot.
        // Preserve that farmer's claim as well as ordinary gather reservations.
        let attended: BTreeSet<_> = s
            .units
            .iter()
            .filter_map(|u| self.economy.builders.get(&u.id).map(|t| t.building))
            .collect();
        let mut farms: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| {
                b.kind == BuildingKind::Farm
                    && b.is_complete()
                    && b.food_remaining.is_some_and(|n| n > 0)
                    && !attended.contains(&b.id)
                    && self
                        .economy
                        .gatherers
                        .count_target(super::super::gathering::WorkTarget::Farm(b.id))
                        == 0
                    && s.avoid
                        .is_none_or(|a| !a.avoid_tile.is_some_and(|t| b.footprint.contains(t)))
            })
            .map(|b| {
                (
                    super::super::dropoff::ring_distance(b.footprint, from),
                    b.id,
                    b.footprint.min,
                )
            })
            .collect();
        farms.sort_unstable_by_key(|b| (b.0, b.1));
        farms.truncate(8);
        farms.sort_by_key(|b| (self.vassal_crosses_water(from, b.2), b.0, b.1));
        for (_, farm, _) in farms {
            if !budget.order(work) {
                break;
            }
            if self.issue_farm(s.player, worker, farm).is_ok() {
                return true;
            }
        }
        false
    }

    pub(super) fn vassal_food(
        &mut self,
        worker: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        if !s.complete {
            return false;
        }
        // A temporary avoidance is not a reason to buy a replacement plot.
        // Another worker can reclaim this one; ordinary admission failures on
        // unrelated, unreachable farms still leave local placement available.
        if let Some(farm) = s.avoid.and_then(|a| a.avoid_tile).and_then(|tile| {
            s.buildings.iter().find(|b| {
                b.kind == BuildingKind::Farm
                    && b.is_complete()
                    && b.food_remaining.is_some_and(|food| food > 0)
                    && b.footprint.contains(tile)
            })
        }) && self
            .economy
            .gatherers
            .count_target(super::super::gathering::WorkTarget::Farm(farm.id))
            == 0
            && !s.units.iter().any(|u| {
                self.economy
                    .builders
                    .get(&u.id)
                    .is_some_and(|b| b.building == farm.id)
            })
        {
            return false;
        }
        let Some(unit) = self.unit(worker) else {
            return false;
        };
        if !s.buildings.iter().any(|b| b.kind == BuildingKind::Mill) {
            return self.vassal_build_near(
                worker,
                BuildingKind::Mill,
                unit.position.tile_floor(),
                s,
                budget,
                work,
            );
        }
        // Finish a new drop-off before opening another cluster.
        if s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Mill && !b.is_complete())
        {
            return false;
        }
        let anchors: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| {
                b.is_complete() && matches!(b.kind, BuildingKind::Mill | BuildingKind::TownCenter)
            })
            .map(|b| (b.id, b.footprint))
            .collect();
        if anchors.is_empty() {
            return false;
        }
        // Each anchor has at most 16 farm lattice cells followed by 8 new
        // Mill sites. The retained cursor reaches every anchor without ever
        // increasing this pass's ordinary placement/order budgets.
        let farm_slots = anchors.len() * 16;
        let total = farm_slots + anchors.len() * 8;
        // Fresh searches begin at the nearest drop-off. Saved indices always
        // refer to the stable entity-ordered list, including a saved zero.
        let start = s.home.farm_after.map_or_else(
            || {
                anchors
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (id, rect))| {
                        (
                            super::super::dropoff::ring_distance(*rect, unit.position.tile_floor()),
                            *id,
                        )
                    })
                    .map_or(0, |(index, _)| index * 16)
            },
            |saved| saved % total,
        );
        for step in 0..total {
            if !budget.site(work) {
                return false;
            }
            let candidate = (start + step) % total;
            s.home.farm_after = Some((candidate + 1) % total);
            if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                home.farm_after = s.home.farm_after;
            }
            let (kind, tile) = if candidate < farm_slots {
                let rect = anchors[candidate / 16].1;
                let columns = (rect.width() + 2) / 3 + 2;
                let rows = (rect.height() + 2) / 3 + 2;
                let slot = (candidate % 16) as i32;
                let (x, y) = (slot % columns, slot / columns);
                if y >= rows || (x > 0 && x < columns - 1 && y > 0 && y < rows - 1) {
                    continue;
                }
                (
                    BuildingKind::Farm,
                    TileCoord::new(rect.min.x + (x - 1) * 3, rect.min.y + (y - 1) * 3),
                )
            } else {
                let offset = candidate - farm_slots;
                let rect = anchors[offset / 8].1;
                let dx = (rect.width() + 2) / 3 * 3 + 6;
                let dy = (rect.height() + 2) / 3 * 3 + 6;
                let (x, y) = [
                    (dx, 0),
                    (0, dy),
                    (-dx, 0),
                    (0, -dy),
                    (dx, dy),
                    (-dx, dy),
                    (dx, -dy),
                    (-dx, -dy),
                ][offset % 8];
                (
                    BuildingKind::Mill,
                    TileCoord::new(rect.min.x + x, rect.min.y + y),
                )
            };
            if self.vassal_farm_site(worker, kind, tile, s, budget, work) {
                // Start with farms again once a cluster has gained a building.
                s.home.farm_after = None;
                if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                    home.farm_after = None;
                }
                return true;
            }
        }
        false
    }

    fn vassal_farm_site(
        &mut self,
        worker: EntityId,
        kind: BuildingKind,
        tile: TileCoord,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        let rules = kind.rules();
        if self
            .player_economy(s.player)
            .is_none_or(|p| p.stockpile.wood < rules.cost.wood)
        {
            return false;
        }
        let rect = TileRect::from_xywh(tile.x, tile.y, rules.side, rules.depth);
        if !s.home.covers(rect, 0)
            || self.validate_explored_footprint(s.player, rect).is_err()
            || self.validate_building_placement(kind, tile).is_err()
            || !self.query(rect).0.is_empty()
            || !budget.order(work)
        {
            return false;
        }
        if let Ok(id) = self.issue_build(s.player, &[worker], kind, tile, false) {
            if let Some(building) = self.building(id) {
                s.buildings.push(building.clone());
            }
            return true;
        }
        false
    }
}
