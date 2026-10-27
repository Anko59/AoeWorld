//! The original Vassal's immutable home, control state and budgeted economy.
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, TileRect, WorldPosition};
use aoe_map::ResourceKind;
use std::collections::{BTreeMap, BTreeSet};
mod construction;
mod hash;
mod jobs;
mod manager;
mod rules;
mod snapshot;
pub use rules::{VASSAL_RULES, VassalRules, VassalWork};

#[derive(Clone, Debug)]
pub(crate) struct Home {
    pub(crate) castle: EntityId,
    // Retained after destruction: a survivor never becomes unbound.
    pub(crate) center: WorldPosition,
    pub(super) workers: BTreeSet<EntityId>,
    pub(super) worker_after: Option<EntityId>,
    pub(super) scan_chunk: usize,
    pub(super) scan_offset: usize,
    pub(super) site_after: usize,
}
impl Home {
    pub(super) fn contains(&self, at: WorldPosition) -> bool {
        let dx = i64::from(at.x) - i64::from(self.center.x);
        let dy = i64::from(at.y) - i64::from(self.center.y);
        let r = i64::from(VASSAL_RULES.radius_tiles * FIXED_SUBUNITS_PER_TILE);
        dx * dx + dy * dy <= r * r
    }
    pub(super) fn covers(&self, rect: TileRect, margin: i32) -> bool {
        [rect.min.x - margin, rect.max.x + margin]
            .into_iter()
            .all(|x| {
                [rect.min.y - margin, rect.max.y + margin]
                    .into_iter()
                    .all(|y| {
                        self.contains(WorldPosition::new(
                            x * FIXED_SUBUNITS_PER_TILE,
                            y * FIXED_SUBUNITS_PER_TILE,
                        ))
                    })
            })
    }
}
#[derive(Debug, Default)]
pub(crate) struct Manager {
    pub(super) active: BTreeSet<EntityId>,
    pub(super) controlled: BTreeMap<EntityId, (EntityId, ResourceKind)>,
    pub(super) cursor: Option<EntityId>,
    /// Only set during an authoritative advance; never across an API boundary.
    pub(crate) automatic: bool,
    pub(super) work: VassalWork,
}
impl GameWorld {
    pub(crate) fn bind_vassal(&mut self, id: EntityId, castle: EntityId, footprint: TileRect) {
        self.economy.vassal_homes.insert(
            id,
            Home {
                castle,
                center: WorldPosition::new(
                    (footprint.min.x + footprint.max.x) * FIXED_SUBUNITS_PER_TILE / 2,
                    (footprint.min.y + footprint.max.y) * FIXED_SUBUNITS_PER_TILE / 2,
                ),
                workers: BTreeSet::new(),
                worker_after: None,
                scan_chunk: 0,
                scan_offset: 0,
                site_after: 0,
            },
        );
    }
    pub fn vassal_castle(&self, id: EntityId) -> Option<EntityId> {
        self.economy.vassal_homes.get(&id).map(|home| home.castle)
    }
    pub(crate) fn vassal_position_allowed(&self, id: EntityId, at: WorldPosition) -> bool {
        // The ordinary-unit hot path needs no ordered-map search.
        if self.unit(id).is_some_and(|u| u.kind != UnitKind::Vassal) {
            return true;
        }
        self.economy
            .vassal_homes
            .get(&id)
            .is_none_or(|home| home.contains(at))
    }
    pub(crate) fn set_vassal_active(&mut self, id: EntityId, active: bool) {
        if active && self.economy.vassal_homes.contains_key(&id) {
            self.economy.vassals.active.insert(id);
        } else {
            self.economy.vassals.active.remove(&id);
            let workers = self
                .economy
                .vassal_homes
                .get_mut(&id)
                .map(|h| std::mem::take(&mut h.workers))
                .unwrap_or_default();
            for worker in workers {
                self.economy.vassals.controlled.remove(&worker);
            }
        }
    }
    pub(crate) fn forget_vassal(&mut self, id: EntityId) {
        self.set_vassal_active(id, false);
        self.economy.vassal_homes.remove(&id);
        self.release_vassal_worker(id);
    }
    fn active_vassal(&self, id: EntityId) -> bool {
        self.economy.vassals.active.contains(&id)
            && self
                .vassal_castle(id)
                .and_then(|c| self.building(c))
                .is_some_and(|b| {
                    b.hp > 0
                        && b.is_complete()
                        && b.garrison.iter().any(|g| g.unit.id == id && g.unit.hp > 0)
                })
    }
    /// The managing Vassal's unit ID, only while this worker is in its active domain.
    pub fn vassal_controller(&self, id: EntityId) -> Option<EntityId> {
        // Frame construction asks for every visible unit. Most have no manager:
        // reject those before another world-unit lookup or shelter inspection.
        let (vassal, _) = *self.economy.vassals.controlled.get(&id)?;
        let unit = self
            .unit(id)
            .filter(|u| u.kind == UnitKind::Villager && u.hp > 0 && !self.embarked(id))?;
        let home = self.economy.vassal_homes.get(&vassal)?;
        (home.contains(unit.position)
            && self.active_vassal(vassal)
            && self
                .building(home.castle)
                .is_some_and(|b| b.player == unit.player))
        .then_some(vassal)
    }
    pub fn vassal_work(&self) -> VassalWork {
        self.economy.vassals.work
    }
    pub(crate) fn player_override(&mut self, id: EntityId) {
        if !self.economy.vassals.automatic {
            self.release_vassal_worker(id);
        }
    }
    fn release_vassal_worker(&mut self, id: EntityId) {
        if let Some((vassal, _)) = self.economy.vassals.controlled.remove(&id)
            && let Some(home) = self.economy.vassal_homes.get_mut(&vassal)
        {
            home.workers.remove(&id);
        }
    }
    fn claim_vassal_worker(&mut self, vassal: EntityId, worker: EntityId, kind: ResourceKind) {
        self.release_vassal_worker(worker);
        if let Some(home) = self.economy.vassal_homes.get_mut(&vassal)
            && home.workers.len() < VASSAL_RULES.max_villagers
        {
            home.workers.insert(worker);
            self.economy
                .vassals
                .controlled
                .insert(worker, (vassal, kind));
        }
    }
    /// Restrict autonomous replacement resources and drop-offs to the domain.
    pub(crate) fn managed_target_allowed(&self, worker: EntityId, at: WorldPosition) -> bool {
        self.vassal_controller(worker)
            .and_then(|v| self.economy.vassal_homes.get(&v))
            .is_none_or(|h| h.contains(at))
    }
}
