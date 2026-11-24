use super::{STEWARD_RULES, StewardWork, rules::Budget, snapshot::Snapshot};
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, TileCoord};
use aoe_map::ResourceKind;
use std::collections::BTreeMap;

// Reserve large startup teams for simultaneous housing and farming shortages.
// An established village keeps the small teams that leave idle workers free
// for other jobs.
fn builder_limit(building: &crate::GameBuilding, starting_village: bool) -> usize {
    match building.kind {
        BuildingKind::Farm => 1,
        BuildingKind::House | BuildingKind::Mill if starting_village => {
            let rect = building.footprint;
            (2 * (rect.width() + rect.height()) + 4) as usize
        }
        BuildingKind::House | BuildingKind::Mill => 3,
        _ => 4,
    }
}

/// Failed job searches for one idle villager: each retry waits twice as long
/// (one to 32 visits); after the second failure the villager is searched
/// again only when the domain's `offer` differs from the one it failed on.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Unemployed {
    pub(super) tries: u8,
    pub(super) retry_at: u64,
    pub(super) offer: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HeldJob {
    pub kind: ResourceKind,
    pub target: super::super::gathering::WorkTarget,
}

impl GameWorld {
    /// What a job search of this domain can find, as one number: the census
    /// state, resource order, which buildings the stockpile pays for, the
    /// housing need, the known buildings (finished or not, free paid Farms),
    /// the discovered nodes and the managed head count. A failed search is
    /// repeated only when this changes: a node discovered, a building placed,
    /// finished or lost, a Farm freed, a zone change, a resource affordable.
    pub(super) fn steward_offer(&self, s: &Snapshot) -> u64 {
        let mix = |h: u64, v: u64| (h ^ v).wrapping_mul(0x0100_0000_01b3).rotate_left(17);
        let mut h = mix(0xcbf2_9ce4_8422_2325, u64::from(s.complete));
        if let Some(priority) = self.steward_priority(s.player, s.home.settings.weights) {
            for kind in priority.kinds() {
                h = mix(h, kind as u64);
            }
        }
        if let Some(p) = self.player_economy(s.player) {
            let a = p.stockpile;
            for kind in [
                BuildingKind::House,
                BuildingKind::Mill,
                BuildingKind::LumberCamp,
                BuildingKind::MiningCamp,
                BuildingKind::Farm,
            ] {
                let c = kind.rules().cost;
                let paid =
                    a.food >= c.food && a.wood >= c.wood && a.gold >= c.gold && a.stone >= c.stone;
                h = mix(h, u64::from(paid));
            }
            let housing =
                p.population_used as usize + s.global_queued + 2 >= p.population_cap as usize;
            h = mix(h, u64::from(housing));
        }
        // Order-independent: a building or node joining or leaving changes
        // the sum, a rotation of the same queue does not.
        let mut set = 0u64;
        for b in &s.buildings {
            let free_farm = b.kind == BuildingKind::Farm
                && b.food_remaining.is_some_and(|food| food > 0)
                && self
                    .economy
                    .gatherers
                    .count_target(super::super::gathering::WorkTarget::Farm(b.id))
                    == 0;
            let key =
                u64::from(b.id.0) << 8 | u64::from(b.is_complete()) << 1 | u64::from(free_farm);
            set = set.wrapping_add(mix(0x9e37_79b9_7f4a_7c15, key));
        }
        h = mix(h, set);
        h = mix(h, s.home.resources.candidate_digest());
        mix(
            h,
            self.economy.steward_homes[&s.steward].workers.len() as u64,
        )
    }

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
            budget.order(work);
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
            if self.steward_job_kind(worker, kind, s, budget, work) {
                return Some(kind);
            }
        }
        None
    }

    fn steward_job_kind(
        &mut self,
        worker: EntityId,
        kind: ResourceKind,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
    ) -> bool {
        // Reuse paid plots before any Food foundation, but never force
        // Food ahead of the kingdom's more urgent available resource.
        if kind == ResourceKind::Food {
            if self.steward_reclaim_farm(worker, s, budget, work) {
                return true;
            }
            if self
                .steward_help_foundation(worker, Some(BuildingKind::Farm), s, budget, work)
                .is_some()
            {
                return true;
            }
        }
        // Wild forage is finite and never counted as farming supply, so a
        // dry-Food retask places a Farm before taking another forage node.
        if kind == ResourceKind::Food && s.farm_first && self.steward_food(worker, s, budget, work)
        {
            return true;
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
                return true;
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
                    return true;
                }
            }
        } else if kind == ResourceKind::Food && self.steward_food(worker, s, budget, work) {
            return true;
        }
        false
    }

    /// Reassign one unloaded gatherer at a time after a severe shortage.
    /// Paid farm jobs, carried loads and pending routes stay with their worker.
    pub(super) fn steward_rebalance(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
        changed: &mut Vec<EntityId>,
    ) {
        const QUOTA_DIVISOR: usize = 4;
        const FOUND_COOLDOWN_SECONDS: u64 = 12;
        const MISSED_COOLDOWN_SECONDS: u64 = 30;
        let now = self.tick.0;
        if now < s.home.rebalance_after || !s.complete || !budget.support_available() {
            return;
        }
        let Some(priority) = self.steward_priority(s.player, s.home.settings.weights) else {
            return;
        };
        let managed = self.economy.steward_homes[&s.steward].workers.len();
        let kinds = priority.kinds();
        if managed == 0
            || !kinds.iter().any(|&wanted| {
                kinds
                    .iter()
                    .any(|&current| priority.severe(wanted, current))
            })
        {
            return;
        }
        let mut working = [0; 4];
        let split = s
            .home
            .rebalance_cursor
            .map_or(0, |id| s.units.partition_point(|u| u.id <= id));
        let mut candidates = Vec::new();
        for u in s.units[split..].iter().chain(&s.units[..split]) {
            if self.steward_controller(u.id) != Some(s.steward) {
                continue;
            }
            if let Some(task) = self.economy.gatherers.get(&u.id) {
                // Only Farms count as long-term Food supply. Preserve the
                // farmer's paid plot and let any loaded worker finish delivery.
                if task.kind != ResourceKind::Food || task.farm.is_some() {
                    working[task.kind as usize] += 1;
                }
                if task.animal.is_none()
                    && task.farm.is_none()
                    && u.carried.is_none()
                    && !u.planning
                    && !self.steward_has_held_job(u.id)
                    && self.economy.steward_homes[&s.steward]
                        .assigned_at
                        .get(&u.id)
                        .is_none_or(|&assigned| {
                            now.saturating_sub(assigned)
                                >= STEWARD_RULES.worker_dwell_seconds
                                    * u64::from(self.config.tick_hz)
                        })
                {
                    candidates.push((u.id, task.kind));
                }
            } else if self.economy.builders.get(&u.id).is_some_and(|task| {
                self.building(task.building)
                    .is_some_and(|b| matches!(b.kind, BuildingKind::Farm | BuildingKind::Mill))
            }) {
                working[ResourceKind::Food as usize] += 1;
            }
        }
        let mut reassigned = false;
        let mut attempted = false;
        for wanted in kinds {
            if working[wanted as usize] >= (managed / QUOTA_DIVISOR).max(1) {
                continue;
            }
            let Some(&(worker, _)) = candidates
                .iter()
                .find(|(_, current)| priority.severe(wanted, *current))
            else {
                continue;
            };
            attempted = true;
            s.home.rebalance_cursor = Some(worker);
            s.avoid = None;
            s.farm_first = wanted == ResourceKind::Food;
            let started = self.steward_job_kind(worker, wanted, s, budget, work);
            s.farm_first = false;
            if started {
                self.claim_steward_worker(s.steward, worker, wanted);
                changed.push(worker);
                reassigned = true;
                break;
            }
        }
        if attempted {
            let cooldown = if reassigned {
                FOUND_COOLDOWN_SECONDS
            } else {
                MISSED_COOLDOWN_SECONDS
            };
            s.home.rebalance_after = now + cooldown * u64::from(self.config.tick_hz);
            if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
                home.rebalance_after = s.home.rebalance_after;
                home.rebalance_cursor = s.home.rebalance_cursor;
            }
        }
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
        let starting_village = s.complete
            && self.player_economy(s.player).is_some_and(|p| {
                p.population_used as usize + s.global_queued >= p.population_cap as usize
            })
            && !self
                .player_buildings_of(s.player, BuildingKind::Mill)
                .any(|b| b.is_complete());
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
                    && !self.economy.dispatch.sites.contains_key(&b.id)
                    && only.map_or(b.kind != BuildingKind::Farm, |kind| b.kind == kind)
                    && attended.get(&b.id).copied().unwrap_or(0)
                        < builder_limit(b, starting_village)
                    && s.home.connects(from, b.footprint)
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
                    builder_limit(b, starting_village),
                )
            })
            .collect();
        unfinished.sort_unstable_by_key(|b| (b.0, b.1));
        unfinished.truncate(8);
        unfinished.sort_by_key(|b| (self.steward_crosses_water(from, b.3), b.0, b.1));
        for (distance, building, kind, _, limit) in unfinished {
            // Prefer the nearest unloaded idle helpers over census-ID order.
            if self.unit(worker)?.carried.is_some() {
                continue;
            }
            let footprint = self.building(building)?.footprint;
            let used = attended.get(&building).copied().unwrap_or(0);
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
            // The water kind alone: a whole composed tile costs a cold
            // landscape evaluation per tile of the line.
            generator
                .water_at_with_cancel(tile, &|| false)
                .ok()
                .flatten()
                .is_some_and(|water| water != aoe_map::WaterKind::None)
        })
    }
}
