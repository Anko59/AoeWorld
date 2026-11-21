//! Building removal also retires its derived production worksets.
use super::*;

impl GameWorld {
    /// Delete a building and its collision (cancelled foundation, spent farm).
    pub(crate) fn remove_building(&mut self, id: EntityId) {
        self.economy.dispatch.sites.remove(&id);
        self.economy.town_bells.remove(&id);
        self.wake_construction(id);
        self.forget_queues(id);
        self.cancel_general_recruit(id);
        self.economy.farm_production.remove(&id);
        self.economy
            .gatherers
            .wake_target(super::super::WorkTarget::Farm(id));
        let Some(building) = self.economy.buildings.remove(&id) else {
            return;
        };
        if building.kind == BuildingKind::Bridge
            && building.is_complete()
            && let Some(span) = building.bridge_span
        {
            self.remove_bridge_deck(span);
        }
        if building.kind == BuildingKind::Road && building.is_complete() {
            self.economy.roads.remove(&building.footprint.min);
        }
        self.economy.active_training.remove(&id);
        self.economy.active_research.remove(&id);
        if building.is_unstarted() {
            self.remove_foundation_reservation(
                id,
                building.player,
                building.footprint,
                building.bridge_span,
            );
        }
        self.forget_building(id, building.footprint);
        if let Some(bucket) = self
            .economy
            .dropoffs
            .get_mut(&(building.player, building.kind))
            && let Ok(at) = bucket.binary_search(&id)
        {
            bucket.remove(at);
        }
        if !building.is_unstarted()
            && let Some(rect) = building
                .kind
                .collision_rect(building.footprint, building.is_complete())
        {
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    let tile = TileCoord::new(x, y);
                    if building.bridge_span.is_none_or(|span| span.contains(tile)) {
                        self.economy.blocked_tiles.remove(&tile);
                    }
                }
            }
            if let Terrain::Map { overlay, .. } = &mut self.terrain {
                if let Some(span) = building.bridge_span {
                    overlay.remove_bridge_foundation(span);
                } else {
                    overlay.remove_footprint(rect);
                }
            }
            if building.kind.is_wall() {
                self.management_wall_changed(rect, false);
            }
            self.collision_opened(building.footprint);
        }
        self.release_destroyed_garrison(id, building.footprint, building.garrison, &mut Vec::new());
        self.economy.open_gates.remove(&id);
        if self.economy.gates.remove(&id) {
            let rect = building.footprint;
            self.management_barrier_changed(rect);
            for y in rect.min.y..rect.max.y {
                for x in rect.min.x..rect.max.x {
                    let tile = TileCoord::new(x, y);
                    if self.economy.gate_tiles.get(&tile) == Some(&id) {
                        self.economy.gate_tiles.remove(&tile);
                    }
                }
            }
        }
        self.combat.tower_ready.remove(&id);
        self.combat.tower_targets.remove(&id);
        self.combat.target_rounds.remove(&id);
        self.refresh_population_cap(building.player);
        self.update_observer(id, None);
        self.remember_visible_buildings();
    }
}
