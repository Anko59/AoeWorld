//! Materialized construction work and completion effects.
use super::*;

impl GameWorld {
    /// `builders + 2` work per tick (one builder adds 3, matching the base
    /// time). Completion memberships are collected once by the caller.
    pub(super) fn add_work(
        &mut self,
        id: EntityId,
        builders: u64,
    ) -> Option<(PlayerId, BuildingKind, TileRect)> {
        self.mark_building_changed(id);
        let building = self.economy.buildings.get_mut(&id)?;
        let total = building.work_total.max(1);
        let max_hp = u64::from(building.max_hp);
        let earned_before = max_hp * building.work_done / total;
        building.work_done = (building.work_done + builders + 2).min(building.work_total);
        // Work adds only the hit points it earns, so combat damage persists.
        let earned = max_hp * building.work_done / total - earned_before;
        building.hp = (u64::from(building.hp) + earned).clamp(1, max_hp) as u16;
        if building.hp == building.max_hp {
            self.combat.hp_quarters[id.0 as usize] = 0;
        }
        if building.work_done < building.work_total {
            return None;
        }
        let (kind, player, footprint, bridge_span) = (
            building.kind,
            building.player,
            building.footprint,
            building.bridge_span,
        );
        if kind == BuildingKind::Bridge
            && bridge_span.is_none_or(|span| !self.complete_bridge(span))
        {
            if let Some(building) = self.economy.buildings.get_mut(&id) {
                building.work_done = building.work_total.saturating_sub(1);
            }
            return None;
        }
        self.economy.buildings.get_mut(&id)?.state = ConstructionState::Complete;
        if kind == BuildingKind::Road {
            self.economy.roads.insert(footprint.min);
        }
        self.eject_from_footprint(kind, footprint);
        if kind == BuildingKind::Farm {
            self.reset_farm_production(id);
        }
        if kind == BuildingKind::TownCenter {
            self.open_town_center_arches(footprint);
        }
        if matches!(kind, BuildingKind::WatchTower | BuildingKind::Castle) {
            self.combat.tower_ready.insert(id, 0);
        }
        self.refresh_population_cap(player);
        Some((player, kind, footprint))
    }
}
