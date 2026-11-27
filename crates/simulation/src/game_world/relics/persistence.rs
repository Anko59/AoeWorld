use super::*;
use crate::{BuildingKind, RestoreReport, SnapshotError};
use std::collections::BTreeSet;

impl GameWorld {
    /// Rebuild derived indexes after all saved units and buildings exist.
    pub(crate) fn restore_relics(&mut self, mut relics: Relics) -> Result<(), SnapshotError> {
        let mut ids = BTreeSet::new();
        let valid_relic = |id: EntityId, ids: &mut BTreeSet<EntityId>| {
            u64::from(id.0) < self.next_entity_id
                && !self.unit_exists(id)
                && !self.economy.buildings.contains_key(&id)
                && self.animal(id).is_none()
                && ids.insert(id)
        };
        let valid = relics
            .ground
            .iter()
            .all(|(&id, &at)| self.config.valid_map_position(at) && valid_relic(id, &mut ids))
            && relics.carriers.iter().all(|(&monk, &id)| {
                self.unit(monk)
                    .is_some_and(|u| u.kind == UnitKind::Monk && u.hp > 0)
                    && valid_relic(id, &mut ids)
            })
            && relics.stored.iter().all(|(&building, held)| {
                lifecycle::MonasteryStore::valid(held)
                    && self
                        .economy
                        .buildings
                        .get(&building)
                        .is_some_and(|b| b.kind == BuildingKind::Monastery && b.is_complete())
                    && held.relics.keys().all(|&id| valid_relic(id, &mut ids))
            });
        if !valid {
            return Err(SnapshotError::Map("invalid relic state".into()));
        }
        relics.rebuild_payments();
        for &id in relics.carriers.keys() {
            self.units[self.lookup[&id]].state.holds_relic = true;
        }
        self.relics = relics;
        Ok(())
    }

    pub(crate) fn resume_relic_orders(&mut self, report: &mut RestoreReport) {
        let pending = std::mem::take(&mut self.relics.orders);
        for (monk, order) in pending {
            let Some(unit) = self.unit(monk) else {
                report.jobs_lost += 1;
                continue;
            };
            let result = match order.target {
                RelicTarget::Pickup(relic) => self.pickup_relic(unit.player, monk, relic, false),
                RelicTarget::Deposit(monastery) => {
                    self.issue_deposit_relic(unit.player, monk, monastery)
                }
            };
            if result.is_ok() {
                report.jobs_resumed += 1;
            } else {
                report.jobs_lost += 1;
            }
        }
    }

    pub(in crate::game_world) fn hash_relics(&self, hash: &mut blake3::Hasher) {
        if self.relics.victory_enabled {
            hash.update(b"relic victory\0");
            match self.relic_victory() {
                None => {
                    hash.update(&[0]);
                }
                Some(RelicVictory::Holding { player, deadline }) => {
                    hash.update(&[1]);
                    hash.update(&player.0.to_le_bytes());
                    hash.update(&deadline.0.to_le_bytes());
                }
                Some(RelicVictory::Won(player)) => {
                    hash.update(&[2]);
                    hash.update(&player.0.to_le_bytes());
                }
            }
        }
        if self.relics.is_empty() {
            return;
        }
        hash.update(b"relics\0");
        hash.update(&(self.relics.ground.len() as u64).to_le_bytes());
        for (id, at) in self.ground_relics() {
            hash.update(&id.0.to_le_bytes());
            hash.update(&at.x.to_le_bytes());
            hash.update(&at.y.to_le_bytes());
        }
        hash.update(&(self.relics.carriers.len() as u64).to_le_bytes());
        for (&monk, &relic) in &self.relics.carriers {
            hash.update(&monk.0.to_le_bytes());
            hash.update(&relic.0.to_le_bytes());
        }
        hash.update(&(self.relics.stored.len() as u64).to_le_bytes());
        for (monastery, site) in &self.relics.stored {
            hash.update(&monastery.0.to_le_bytes());
            hash.update(&(site.relics.len() as u64).to_le_bytes());
            for (relic, due) in &site.relics {
                hash.update(&relic.0.to_le_bytes());
                hash.update(&due.to_le_bytes());
            }
        }
        hash.update(&(self.relics.orders.len() as u64).to_le_bytes());
        for (monk, order) in &self.relics.orders {
            hash.update(&monk.0.to_le_bytes());
            match order.target {
                RelicTarget::Pickup(relic) => {
                    hash.update(&[0]);
                    hash.update(&relic.0.to_le_bytes());
                }
                RelicTarget::Deposit(monastery) => {
                    hash.update(&[1]);
                    hash.update(&monastery.0.to_le_bytes());
                }
            }
            if let Some(at) = order.approach {
                hash.update(&[1]);
                hash.update(&at.x.to_le_bytes());
                hash.update(&at.y.to_le_bytes());
            } else {
                hash.update(&[0]);
            }
        }
    }
}
