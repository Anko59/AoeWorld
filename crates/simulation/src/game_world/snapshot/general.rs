//! Restore explicit ownership after all identities exist, never by proximity.
use super::{BuildingRecord, GameUnit, GameWorld, UnitRecord};
use aoe_core::{EntityId, PlayerId};
use std::collections::BTreeMap;

impl GameWorld {
    fn restored_general_unit(&self, record: &UnitRecord) -> Option<GameUnit> {
        self.unit(record.unit.id)
            .or_else(|| {
                record
                    .garrison_in
                    .and_then(|b| self.building(b))
                    .and_then(|b| {
                        b.garrison
                            .iter()
                            .find(|g| g.unit.id == record.unit.id)
                            .map(|g| g.unit)
                    })
            })
            .filter(|u| u.hp > 0)
    }

    fn restored_general_owners(&self, records: &[UnitRecord]) -> BTreeMap<EntityId, PlayerId> {
        records
            .iter()
            .filter(|r| r.unit.kind == crate::UnitKind::General)
            .filter_map(|r| self.restored_general_unit(r))
            .filter(|u| u.kind == crate::UnitKind::General)
            .map(|u| (u.id, u.player))
            .collect()
    }

    pub(super) fn restore_general_recruits(
        &mut self,
        records: &[BuildingRecord],
        units: &[UnitRecord],
    ) {
        let owners = self.restored_general_owners(units);
        for record in records {
            let Some((general, kind)) = record.general_recruit else {
                continue;
            };
            let Some(&owner) = owners.get(&general) else {
                continue;
            };
            if !self.building(record.id).is_some_and(|b| {
                b.player == owner
                    && b.hp > 0
                    && b.is_complete()
                    && b.training_queue
                        .front()
                        .is_some_and(|entry| entry.kind == kind)
            }) {
                continue;
            }
            let Some(army) = self.economy.generals.armies.get_mut(&general) else {
                continue;
            };
            if !army.room_for(kind) {
                continue;
            }
            army.pending.insert(record.id, kind);
            self.economy
                .generals
                .recruits
                .insert(record.id, (general, kind));
        }
    }

    pub(super) fn restore_general_ownership(&mut self, records: &[UnitRecord]) {
        let owners = self.restored_general_owners(records);
        // Spawning a saved General may have censused earlier-id units. That
        // incidental membership is not evidence of the owner's saved intent.
        let claimed: Vec<_> = self.economy.generals.controlled.keys().copied().collect();
        for id in claimed {
            self.forget_general(id);
        }
        for record in records {
            let id = record.unit.id;
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].state.sergeant = false;
            } else if let Some(shelter) = record.garrison_in.and_then(|b| self.building_mut(b))
                && let Some(held) = shelter.garrison.iter_mut().find(|g| g.unit.id == id)
            {
                held.unit.sergeant = false;
            }
        }
        for record in records {
            let id = record.unit.id;
            let Some(unit) = self.restored_general_unit(record) else {
                continue;
            };
            if let Some(ordered) = record.general_order_tick {
                self.economy.generals.manual.insert(id, ordered);
            } else if let Some(general) = record.general_controller
                && owners.get(&general) == Some(&unit.player)
            {
                // Reuse the army's role, capacity and existing-controller checks.
                // Embarked flags are restored after this association pass.
                self.claim_general_unit(general, unit);
            }
        }
    }
}
