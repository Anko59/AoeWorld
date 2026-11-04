//! A Steward takes over the owned villagers in its territory, busy ones
//! included. Accepted owner overrides remain manual until real idle.
use super::snapshot::Snapshot;
use crate::{GameUnit, GameWorld};
use aoe_core::WorldPosition;
use aoe_map::ResourceKind;

impl GameWorld {
    /// Bookkeeping only (no orders, no search): bounded by the snapshot and the
    /// 100-worker cap. Idle villagers are adopted by the job pass instead, which
    /// also gives them work.
    pub(super) fn steward_adopt(&mut self, s: &Snapshot) {
        // Clean only entries seen by this bounded census, even at the cap.
        // A manual worker's load may remain, but no active owner job may remain.
        if !self.economy.stewards.manual.is_empty() {
            for u in &s.units {
                if self.economy.stewards.manual.contains(&u.id)
                    && !u.moving
                    && !u.planning
                    && matches!(u.task, crate::UnitTask::Idle | crate::UnitTask::Carrying)
                    && !self.economy.gatherers.contains_key(&u.id)
                    && !self.economy.builders.contains_key(&u.id)
                    && !self.economy.garrison_orders.contains_key(&u.id)
                {
                    self.economy.stewards.manual.remove(&u.id);
                }
            }
        }
        for u in &s.units {
            if self.economy.stewards.manual.contains(&u.id) {
                continue;
            }
            let full = self
                .economy
                .steward_homes
                .get(&s.steward)
                .is_none_or(|h| h.workers.len() >= h.cap());
            if full {
                break;
            }
            if self.economy.stewards.controlled.contains_key(&u.id) {
                continue;
            }
            if let Some(kind) = self.steward_busy_kind(u, s) {
                self.claim_steward_worker(s.steward, u.id, kind);
            }
        }
    }
    /// The resource a busy villager is serving, or `None` for a worker the
    /// owner is steering by hand (plain move, garrison order), that is idle,
    /// or that is crossing this territory to serve a different domain's job.
    fn steward_busy_kind(&self, u: &GameUnit, s: &Snapshot) -> Option<ResourceKind> {
        if self.economy.garrison_orders.contains_key(&u.id) {
            return None;
        }
        if let Some(task) = self.economy.gatherers.get(&u.id) {
            let target = WorldPosition::from_tile_center(task.tile).ok()?;
            return s.home.owns(target).then_some(task.kind);
        }
        let task = self.economy.builders.get(&u.id)?;
        let building = self.building(task.building)?;
        s.home
            .covers(building.footprint, 0)
            .then_some(ResourceKind::Wood)
    }
}
