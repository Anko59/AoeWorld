//! Read-only global counts, independent of visibility and spatial residency.
use crate::{GameUnit, GameWorld};
use aoe_core::PlayerId;
use std::collections::BTreeMap;

impl GameWorld {
    /// Living units by native kind, including passengers, shelters and blocked
    /// shelter exits. This derived report does not mutate canonical state.
    pub fn unit_census(&self) -> BTreeMap<PlayerId, [u32; crate::UNIT_COMBAT.len()]> {
        let mut counts = BTreeMap::new();
        let mut add = |u: &GameUnit| {
            if u.hp > 0 {
                let row = counts
                    .entry(u.player)
                    .or_insert([0_u32; crate::UNIT_COMBAT.len()]);
                row[u.kind as usize] = row[u.kind as usize].saturating_add(1);
            }
        };
        for unit in &self.units {
            add(&unit.state);
        }
        for building in self.economy.buildings.values() {
            for occupant in &building.garrison {
                add(&occupant.unit);
            }
        }
        for (_, occupants) in self.economy.ejections.values() {
            for occupant in occupants {
                add(&occupant.unit);
            }
        }
        counts
    }
}
