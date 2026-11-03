//! Bounded coastal knowledge and a separate fleet, sharing domain work limits.
use super::{VASSAL_RULES, VassalWork, rules::Budget, snapshot::Snapshot};
use crate::{GameWorld, Terrain, TileVisibility, UnitKind, UnitTask};
use aoe_core::{EntityId, WorldPosition};
use aoe_map::{ObjectKind, ResourceNode};
use std::collections::BTreeSet;
mod docks;
mod ships;

const KNOWN_FISH: usize = 8;

#[derive(Clone, Debug, Default)]
pub(super) struct Fleet {
    pub(super) ships: BTreeSet<EntityId>,
    fish: Vec<ResourceNode>,
    after: Option<EntityId>,
    site_after: usize,
}

impl Fleet {
    pub(super) fn update_hash(&self, h: &mut blake3::Hasher) {
        h.update(&(self.ships.len() as u64).to_le_bytes());
        for id in &self.ships {
            h.update(&id.0.to_le_bytes());
        }
        h.update(&self.after.map_or(u32::MAX, |id| id.0).to_le_bytes());
        h.update(&(self.site_after as u64).to_le_bytes());
        h.update(&(self.fish.len() as u64).to_le_bytes());
        for node in &self.fish {
            h.update(&node.id.to_le_bytes());
        }
    }
}

impl GameWorld {
    fn vassal_fish_known(&self, node: ResourceNode, s: &Snapshot) -> bool {
        matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish)
            && WorldPosition::from_tile_center(node.tile).is_ok_and(|at| s.home.owns(at))
            && self.visibility(s.player, node.tile) != TileVisibility::Unexplored
            && self.resource_remaining(node.id).is_some_and(|n| n > 0)
    }

    pub(in crate::game_world::economy::vassal) fn vassal_remember_fish(
        &mut self,
        node: ResourceNode,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) {
        if !self.vassal_fish_known(node, s) || !budget.inspect(work, s.home.navy.fish.len()) {
            return;
        }
        if s.home.navy.fish.iter().any(|old| old.id == node.id) {
            return;
        }
        if s.home.navy.fish.len() == KNOWN_FISH {
            s.home.navy.fish.remove(0);
        }
        s.home.navy.fish.push(node);
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.navy.fish = s.home.navy.fish.clone();
        }
    }

    pub(in crate::game_world::economy::vassal) fn vassal_navy(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        let fish = std::mem::take(&mut s.home.navy.fish);
        s.home.navy.fish = fish
            .into_iter()
            .filter(|node| budget.inspect(work, 1) && self.vassal_fish_known(*node, s))
            .collect();
        if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
            home.navy.fish = s.home.navy.fish.clone();
        }
        // A manually steered ship stays manual until its real owner job ends.
        for boat in &s.boats {
            if !boat.moving
                && !boat.planning
                && matches!(boat.task, UnitTask::Idle | UnitTask::Carrying)
                && !self.economy.gatherers.contains_key(&boat.id)
                && !self.economy.garrison_orders.contains_key(&boat.id)
            {
                self.economy.vassals.manual.remove(&boat.id);
            }
        }
        self.vassal_fleet_jobs(s, budget, work, changed);
        self.vassal_docks(s, budget, work, changed);
    }

    fn vassal_boat_idle(&self, id: EntityId) -> bool {
        self.unit(id).is_some_and(|u| {
            u.kind == UnitKind::FishingShip
                && u.hp > 0
                && !u.moving
                && !u.planning
                && matches!(u.task, UnitTask::Idle | UnitTask::Carrying)
        }) && !self.economy.garrison_orders.contains_key(&id)
            && !self.economy.gatherers.contains_key(&id)
            && !self.economy.vassals.manual.contains(&id)
    }
}
