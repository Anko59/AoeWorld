//! Each sheltered identity keeps its ordinary healing deadline.
use crate::GameWorld;
use aoe_core::EntityId;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(crate) struct Timers {
    due: BTreeSet<(u64, EntityId, EntityId)>,
    indexed: BTreeMap<EntityId, BTreeMap<EntityId, u64>>,
    awake: BTreeSet<EntityId>,
}

impl Timers {
    fn erase(&mut self, building: EntityId) {
        if let Some(units) = self.indexed.remove(&building) {
            for (unit, tick) in units {
                self.due.remove(&(tick, building, unit));
            }
        }
    }

    pub(crate) fn remove(&mut self, building: EntityId) {
        self.erase(building);
        self.awake.remove(&building);
    }

    fn wake(&mut self, building: EntityId) {
        self.erase(building);
        self.awake.insert(building);
    }

    pub(crate) fn reset(&mut self, buildings: impl Iterator<Item = EntityId>) {
        self.due.clear();
        self.indexed.clear();
        self.awake.clear();
        self.awake.extend(buildings);
    }

    fn schedule(&mut self, building: EntityId, unit: EntityId, tick: u64) {
        if let Some(old) = self.indexed.entry(building).or_default().insert(unit, tick) {
            self.due.remove(&(old, building, unit));
        }
        self.due.insert((tick, building, unit));
    }

    fn ready(&mut self, now: u64) -> BTreeMap<EntityId, BTreeSet<EntityId>> {
        let mut ready = BTreeMap::<_, BTreeSet<_>>::new();
        while let Some(&(tick, building, unit)) = self.due.first() {
            if tick > now {
                break;
            }
            self.due.pop_first();
            ready.entry(building).or_default().insert(unit);
        }
        ready
    }
}

impl GameWorld {
    pub(crate) fn wake_garrison(&mut self, id: EntityId) {
        if self.economy.occupied_garrisons.contains(&id) {
            self.economy.garrison_timers.wake(id);
        }
    }

    pub(super) fn advance_garrison_healing(&mut self) {
        #[cfg(test)]
        if !self.economy.no_sleep {
            self.assert_garrison_timers();
        }
        let ready: BTreeMap<_, BTreeSet<_>> = if self.economy.no_sleep {
            self.economy
                .occupied_garrisons
                .iter()
                .filter_map(|id| {
                    let b = self.economy.buildings.get(id)?;
                    Some((*id, b.garrison.iter().map(|g| g.unit.id).collect()))
                })
                .collect()
        } else {
            for id in std::mem::take(&mut self.economy.garrison_timers.awake) {
                self.economy.garrison_timers.erase(id);
                if let Some(b) = self.economy.buildings.get(&id) {
                    for g in &b.garrison {
                        self.economy
                            .garrison_timers
                            .schedule(id, g.unit.id, g.heal_at);
                    }
                }
            }
            self.economy.garrison_timers.ready(self.tick.0)
        };
        for (id, units) in ready {
            self.mark_building_changed(id);
            let Some(b) = self.economy.buildings.get_mut(&id) else {
                self.economy.garrison_timers.remove(id);
                continue;
            };
            let mut period = b.kind.heal_seconds() * u64::from(self.config.tick_hz);
            if self
                .economy
                .players
                .get(&b.player)
                .is_some_and(|p| p.researched & crate::Tech::HerbalMedicine.bit() != 0)
            {
                period = (period / 6).max(1);
            }
            for unit in units {
                let Ok(at) = b.garrison.binary_search_by_key(&unit, |g| g.unit.id) else {
                    self.economy.garrison_timers.wake(id);
                    continue;
                };
                let g = &mut b.garrison[at];
                #[cfg(test)]
                {
                    self.economy.tick_work.garrison_units += 1;
                }
                if self.tick.0 >= g.heal_at {
                    g.unit.hp = g.unit.hp.saturating_add(1).min(g.unit.max_hp);
                    if g.unit.hp == g.unit.max_hp {
                        self.combat.hp_quarters[g.unit.id.0 as usize] = 0;
                    }
                    g.heal_at = self.tick.0 + period;
                }
                if !self.economy.no_sleep {
                    self.economy.garrison_timers.schedule(id, unit, g.heal_at);
                }
            }
        }
    }

    #[cfg(test)]
    fn assert_garrison_timers(&self) {
        let timers = &self.economy.garrison_timers;
        for id in &self.economy.occupied_garrisons {
            if timers.awake.contains(id) {
                continue;
            }
            let b = &self.economy.buildings[id];
            let expected: BTreeMap<_, _> =
                b.garrison.iter().map(|g| (g.unit.id, g.heal_at)).collect();
            assert_eq!(
                timers.indexed.get(id),
                Some(&expected),
                "missed wake: garrison {id:?} clocks"
            );
            for (&unit, &tick) in &expected {
                assert!(
                    timers.due.contains(&(tick, *id, unit)),
                    "missed wake: garrison {id:?} deadline"
                );
            }
        }
        for id in timers.indexed.keys() {
            assert!(
                self.economy.occupied_garrisons.contains(id),
                "missed wake: garrison {id:?} removed"
            );
        }
    }
}
