use super::*;
use crate::BuildingKind;
use aoe_core::{TileCoord, TileRect};
use aoe_map::{ResourceKind, WaterKind};

impl GameWorld {
    pub(super) fn steward_docks(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
        changed: &mut Vec<EntityId>,
    ) {
        if !s.complete || s.home.navy.fish.is_empty() {
            return;
        }
        // The census already restricts buildings to this owned territory.
        // Reuse a paid foundation too; never buy a second Dock while it builds.
        if let Some(dock) = s
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Dock)
            .cloned()
        {
            if !dock.is_complete() {
                let attended = s.units.iter().any(|u| {
                    self.economy
                        .builders
                        .get(&u.id)
                        .is_some_and(|task| task.building == dock.id)
                });
                if !attended
                    && let Some(worker) = self.steward_dock_builder(s, Some(dock.footprint))
                    && budget.order(work)
                    && self.issue_construct(s.player, &[worker], dock.id).is_ok()
                {
                    self.claim_steward_worker(s.steward, worker, ResourceKind::Food);
                    changed.push(worker);
                }
                return;
            }
            let queued: usize = s
                .buildings
                .iter()
                .flat_map(|b| &b.training_queue)
                .filter(|e| e.kind == UnitKind::FishingShip)
                .count();
            let Some(player) = self.player_economy(s.player) else {
                return;
            };
            if s.home.settings.training
                && self
                    .steward_priority(s.player, s.home.settings.weights)
                    .is_some_and(|p| p.kinds()[0] == ResourceKind::Food)
                && s.boats.len() + queued < STEWARD_RULES.max_villagers
                // Boats that cannot fish are no food supply: never add more.
                && !s.boats.iter().any(|b| self.steward_boat_unproductive(b.id))
                && dock.training_queue.is_empty()
                && dock.research.is_none()
                && player.stockpile.wood >= 75
                && budget.order(work)
            {
                let _ = self.issue_train(s.player, dock.id, UnitKind::FishingShip);
            }
            return;
        }
        let Some(worker) = self.steward_dock_builder(s, None) else {
            return;
        };
        if self
            .player_economy(s.player)
            .is_none_or(|p| p.stockpile.wood < BuildingKind::Dock.rules().cost.wood)
        {
            return;
        }
        // A known fish-to-TownCenter ray supplies a bounded shore anchor; ordinary
        // explored placement and the builder's route admission decide legality.
        let Some(near) = s
            .home
            .navy
            .fish
            .iter()
            .find_map(|n| self.steward_shore(n.tile, s))
        else {
            return;
        };
        let start = s.home.navy.site_after;
        for step in 0..81 {
            if !budget.site(work) {
                break;
            }
            let candidate = (start + step) % 81;
            let next = (candidate + 1) % 81;
            s.home.navy.site_after = next;
            if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
                home.navy.site_after = next;
            }
            let tile = TileCoord::new(
                near.x + (candidate % 9) as i32 - 4,
                near.y + (candidate / 9) as i32 - 4,
            );
            let rect = TileRect::from_xywh(tile.x, tile.y, 3, 3);
            if !s.home.covers(rect, 1)
                || !self.steward_site_walkable(worker, rect, s)
                || self.validate_explored_footprint(s.player, rect).is_err()
                || self
                    .validate_building_placement(BuildingKind::Dock, tile)
                    .is_err()
            {
                continue;
            }
            if !budget.order(work) {
                break;
            }
            if let Ok(id) = self.issue_build(s.player, &[worker], BuildingKind::Dock, tile, false) {
                self.claim_steward_worker(s.steward, worker, ResourceKind::Food);
                changed.push(worker);
                if let Some(building) = self.building(id) {
                    s.buildings.push(building.into_owned());
                }
                break;
            }
        }
    }

    /// A boat with no fishing job, or one labelled Moving that does not move.
    fn steward_boat_unproductive(&self, id: EntityId) -> bool {
        self.unit(id).is_some_and(|u| {
            u.hp > 0
                && !self.economy.garrison_orders.contains_key(&id)
                && (!self.economy.gatherers.contains_key(&id)
                    || (u.task == crate::UnitTask::Moving && !u.moving && !u.planning))
        })
    }

    /// A free villager; for an existing foundation it must also be able to walk
    /// to it without leaving the territory.
    fn steward_dock_builder(&self, s: &Snapshot, site: Option<TileRect>) -> Option<EntityId> {
        s.units
            .iter()
            .filter_map(|u| self.unit(u.id))
            .find(|u| {
                !self.economy.stewards.manual.contains(&u.id)
                    && !self.economy.builders.contains_key(&u.id)
                    && site.is_none_or(|rect| s.home.connects(u.position.tile_floor(), rect))
                    && (self.steward_worker_idle(u.id, s.steward)
                        || (self.steward_controller(u.id) == Some(s.steward)
                            && u.carried.is_none()))
            })
            .map(|u| u.id)
    }

    fn steward_shore(&self, fish: TileCoord, s: &Snapshot) -> Option<TileCoord> {
        let Terrain::Map { generator, .. } = &self.terrain else {
            return None;
        };
        let center = s.home.center.tile_floor();
        let (dx, dy) = (center.x - fish.x, center.y - fish.y);
        let steps = dx.abs().max(dy.abs()).min(2 * STEWARD_RULES.radius_tiles);
        for i in 1..=steps {
            let tile = TileCoord::new(fish.x + dx * i / steps, fish.y + dy * i / steps);
            if generator
                .tile_at(tile)
                .is_some_and(|t| t.water == WaterKind::None && t.passable)
            {
                return Some(tile);
            }
        }
        None
    }
}
