use super::{StewardWork, rules::Budget, snapshot::Snapshot};
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, TileCoord};
use aoe_map::ResourceKind;
use std::collections::BTreeMap;

// Farms need one occupant; small houses and mills use a team of three.
// Camps keep a four-worker team so the first useful drop-off opens promptly.
fn builder_limit(kind: BuildingKind) -> usize {
    match kind {
        BuildingKind::Farm => 1,
        BuildingKind::House | BuildingKind::Mill => 3,
        _ => 4,
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HeldJob {
    pub kind: ResourceKind,
    pub target: super::super::gathering::WorkTarget,
}

impl GameWorld {
    /// Ordinary automatic construction borrows a worker, never its profession.
    pub(crate) fn steward_hold_job(&mut self, worker: EntityId) {
        if !self.economy.stewards.automatic {
            return;
        }
        let Some(steward) = self.steward_controller(worker) else {
            return;
        };
        let Some(task) = self.economy.gatherers.get(&worker) else {
            return;
        };
        use super::super::gathering::WorkTarget;
        let job = HeldJob {
            kind: task.kind,
            target: task
                .animal
                .map(WorkTarget::Animal)
                .or_else(|| task.farm.map(WorkTarget::Farm))
                .unwrap_or(WorkTarget::Node(task.node)),
        };
        if let Some(home) = self.economy.steward_homes.get_mut(&steward) {
            home.held_jobs.insert(worker, job);
        }
    }

    pub(crate) fn steward_has_held_job(&self, worker: EntityId) -> bool {
        self.steward_controller(worker).is_some_and(|steward| {
            self.economy.steward_homes[&steward]
                .held_jobs
                .contains_key(&worker)
        })
    }

    pub(super) fn steward_job(
        &mut self,
        worker: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
    ) -> Option<ResourceKind> {
        if let Some(job) = self.economy.steward_homes[&s.steward]
            .held_jobs
            .get(&worker)
            .copied()
        {
            use super::super::gathering::WorkTarget;
            if !budget.order(work) {
                return None;
            }
            let accepted = match job.target {
                WorkTarget::Farm(farm) => self.issue_farm(s.player, worker, farm).is_ok(),
                WorkTarget::Node(node) => self.issue_gather(s.player, worker, node).is_ok(),
                WorkTarget::Animal(animal) => self.begin_carcass_gather(worker, animal).is_ok(),
            };
            self.economy
                .steward_homes
                .get_mut(&s.steward)?
                .held_jobs
                .remove(&worker);
            if accepted {
                return Some(job.kind);
            }
        }
        let unit = self.unit(worker)?;
        let priority = self.steward_priority(s.player, s.home.settings.weights)?;
        let kinds = priority.kinds();
        if kinds[0] == ResourceKind::Food
            && unit
                .carried
                .is_none_or(|load| load.kind == ResourceKind::Food)
            && s.avoid
                .is_none_or(|a| a.avoid_kind != Some(ResourceKind::Food))
            && self.steward_reclaim_farm(worker, s, budget, work)
        {
            return Some(ResourceKind::Food);
        }
        // General construction and housing may use idle helpers. A Farm
        // foundation is considered only when Food wins resource selection.
        let p = self.player_economy(s.player)?;
        if s.complete
            && p.population_used as usize + s.global_queued + 2 >= p.population_cap as usize
            && !s
                .buildings
                .iter()
                .any(|b| b.kind == BuildingKind::House && !b.is_complete())
            && self.steward_build_near(
                worker,
                BuildingKind::House,
                unit.position.tile_floor(),
                s,
                budget,
                work,
            )
        {
            return Some(ResourceKind::Wood);
        }
        if let Some(kind) = self.steward_help_foundation(worker, None, s, budget, work) {
            return Some(kind);
        }
        for kind in kinds {
            if unit.carried.is_some_and(|c| c.kind != kind)
                || s.avoid.is_some_and(|a| a.avoid_kind == Some(kind))
            {
                continue;
            }
            // Reuse paid plots before any Food foundation, but never force
            // Food ahead of the kingdom's more urgent available resource.
            if kind == ResourceKind::Food {
                if self.steward_reclaim_farm(worker, s, budget, work) {
                    return Some(kind);
                }
                if self
                    .steward_help_foundation(worker, Some(BuildingKind::Farm), s, budget, work)
                    .is_some()
                {
                    return Some(kind);
                }
            }
            if let Some(node) = self.steward_resource(worker, kind, s, budget, work) {
                let close = s.buildings.iter().any(|b| {
                    b.is_complete()
                        && b.kind.accepts(kind)
                        && super::super::dropoff::ring_distance(b.footprint, node.tile) <= 64
                });
                let camp = match kind {
                    ResourceKind::Food => BuildingKind::Mill,
                    ResourceKind::Wood => BuildingKind::LumberCamp,
                    ResourceKind::Gold | ResourceKind::Stone => BuildingKind::MiningCamp,
                };
                if !close && self.steward_build_near(worker, camp, node.tile, s, budget, work) {
                    return Some(kind);
                }
                // A far existing camp remains useful when a nearer one cannot
                // be afforded. No drop-off at all is not a usable gather job.
                if s.buildings
                    .iter()
                    .any(|b| b.is_complete() && b.kind.accepts(kind))
                    && budget.order(work)
                {
                    let accepted = self.issue_gather(s.player, worker, node.id).is_ok();
                    self.steward_resource_result(worker, kind, node.id, accepted, s);
                    if accepted {
                        return Some(kind);
                    }
                }
            } else if kind == ResourceKind::Food && self.steward_food(worker, s, budget, work) {
                return Some(kind);
            }
        }
        None
    }

    fn steward_help_foundation(
        &mut self,
        worker: EntityId,
        only: Option<BuildingKind>,
        s: &Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
    ) -> Option<ResourceKind> {
        let from = self.unit(worker)?.position.tile_floor();
        // Read live tasks: earlier assignments in this pass already occupy a
        // place, even though the unit census predates those orders. A partial
        // census must also count managed builders seen in an earlier slice.
        let mut attended = BTreeMap::<EntityId, usize>::new();
        let unseen = self.economy.steward_homes[&s.steward]
            .workers
            .iter()
            .copied()
            .filter(|id| s.units.binary_search_by_key(id, |u| u.id).is_err());
        for worker in s.units.iter().map(|u| u.id).chain(unseen) {
            if let Some(task) = self.economy.builders.get(&worker) {
                *attended.entry(task.building).or_default() += 1;
            }
        }
        let mut unfinished: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| {
                !b.is_complete()
                    && only.map_or(b.kind != BuildingKind::Farm, |kind| b.kind == kind)
                    && attended.get(&b.id).copied().unwrap_or(0) < builder_limit(b.kind)
                    && matches!(
                        b.kind,
                        BuildingKind::House
                            | BuildingKind::LumberCamp
                            | BuildingKind::MiningCamp
                            | BuildingKind::Mill
                            | BuildingKind::Farm
                    )
            })
            .map(|b| {
                (
                    super::super::dropoff::ring_distance(b.footprint, from),
                    b.id,
                    b.kind,
                    b.footprint.min,
                )
            })
            .collect();
        unfinished.sort_unstable_by_key(|b| (b.0, b.1));
        unfinished.truncate(8);
        unfinished.sort_by_key(|b| (self.steward_crosses_water(from, b.3), b.0, b.1));
        for (distance, building, kind, _) in unfinished {
            // Prefer the nearest unloaded idle helpers over census-ID order.
            if self.unit(worker)?.carried.is_some() {
                continue;
            }
            let footprint = self.building(building)?.footprint;
            let used = attended.get(&building).copied().unwrap_or(0);
            let limit = builder_limit(kind);
            let nearer = s
                .units
                .iter()
                .filter(|u| {
                    self.steward_worker_idle(u.id, s.steward)
                        && self.unit(u.id).is_some_and(|live| {
                            live.carried.is_none()
                                && (
                                    super::super::dropoff::ring_distance(
                                        footprint,
                                        live.position.tile_floor(),
                                    ),
                                    live.id,
                                ) < (distance, worker)
                        })
                })
                .take(limit - used)
                .count();
            if nearer >= limit - used {
                continue;
            }
            if budget.order(work) && self.issue_construct(s.player, &[worker], building).is_ok() {
                return Some(match kind {
                    BuildingKind::Mill | BuildingKind::Farm => ResourceKind::Food,
                    BuildingKind::MiningCamp => ResourceKind::Gold,
                    _ => ResourceKind::Wood,
                });
            }
        }
        None
    }

    // A straight water crossing is a conservative preference, not a route
    // certificate: normal order admission remains responsible for reachability.
    pub(super) fn steward_crosses_water(&self, from: TileCoord, to: TileCoord) -> bool {
        let crate::Terrain::Map { generator, .. } = &self.terrain else {
            return false;
        };
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let steps = dx.abs().max(dy.abs());
        (1..steps).any(|i| {
            let tile = TileCoord::new(from.x + dx * i / steps, from.y + dy * i / steps);
            generator
                .tile_at(tile)
                .is_some_and(|t| t.water != aoe_map::WaterKind::None)
        })
    }
}
