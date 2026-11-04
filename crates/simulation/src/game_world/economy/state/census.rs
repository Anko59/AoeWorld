//! Read-only global counts, independent of visibility and spatial residency.
use crate::{GameUnit, GameWorld};
use aoe_core::PlayerId;
use std::collections::BTreeMap;
use std::sync::Arc;

type Counts = BTreeMap<PlayerId, [u32; crate::UNIT_COMBAT.len()]>;

#[derive(Debug, Default)]
pub(crate) struct Cache {
    value: Option<Arc<Counts>>,
    rebuilds: u64,
}

impl GameWorld {
    /// Living units by native kind, including passengers, shelters and blocked
    /// shelter exits. This derived report does not mutate canonical state.
    pub fn unit_census(&self) -> Arc<Counts> {
        let mut cache = self
            .economy
            .census
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(value) = &cache.value {
            return Arc::clone(value);
        }
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
        let counts = Arc::new(counts);
        cache.value = Some(Arc::clone(&counts));
        cache.rebuilds += 1;
        counts
    }

    /// Derived work counter, excluded from canonical simulation state.
    pub fn unit_census_rebuilds(&self) -> u64 {
        self.economy
            .census
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rebuilds
    }

    /// Keep a materialized report current without another unit scan.
    pub(crate) fn change_unit_census(
        &mut self,
        player: PlayerId,
        from: Option<crate::UnitKind>,
        to: Option<crate::UnitKind>,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        let cache = self
            .economy
            .census
            .get_mut()
            .unwrap_or_else(|e| e.into_inner());
        let Some(value) = &mut cache.value else {
            return;
        };
        let rows = Arc::make_mut(value);
        let row = rows.entry(player).or_insert([0; crate::UNIT_COMBAT.len()]);
        if let Some(kind) = from {
            row[kind as usize] = row[kind as usize].saturating_sub(count);
        }
        if let Some(kind) = to {
            row[kind as usize] = row[kind as usize].saturating_add(count);
        }
        if row.iter().all(|n| *n == 0) {
            rows.remove(&player);
        }
    }
}
