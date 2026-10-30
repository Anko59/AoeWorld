//! A Vassal takes over the owned villagers in its territory, busy ones
//! included: they finish their current job and are retasked only once idle.
use super::VASSAL_RULES;
use super::snapshot::Snapshot;
use crate::{GameUnit, GameWorld};
use aoe_map::ResourceKind;

impl GameWorld {
    /// Bookkeeping only (no orders, no search): bounded by the snapshot and the
    /// 100-worker cap. Idle villagers are adopted by the job pass instead, which
    /// also gives them work.
    pub(super) fn vassal_adopt(&mut self, s: &Snapshot) {
        for u in &s.units {
            let full = self
                .economy
                .vassal_homes
                .get(&s.vassal)
                .is_none_or(|h| h.workers.len() >= VASSAL_RULES.max_villagers);
            if full {
                break;
            }
            if self.economy.vassals.controlled.contains_key(&u.id) {
                continue;
            }
            if let Some(kind) = self.vassal_busy_kind(u) {
                self.claim_vassal_worker(s.vassal, u.id, kind);
            }
        }
    }
    /// The resource a busy villager is serving, or `None` for a worker the
    /// owner is steering by hand (plain move, garrison order) or that is idle.
    fn vassal_busy_kind(&self, u: &GameUnit) -> Option<ResourceKind> {
        if self.economy.garrison_orders.contains_key(&u.id) {
            return None;
        }
        self.gathering_kind(u.id).or_else(|| {
            self.economy
                .builders
                .contains_key(&u.id)
                .then_some(ResourceKind::Wood)
        })
    }
}
