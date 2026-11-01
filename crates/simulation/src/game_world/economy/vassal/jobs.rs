use super::{VassalWork, rules::Budget, snapshot::Snapshot};
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, TileCoord};
use aoe_map::ResourceKind;
use std::collections::BTreeSet;

impl GameWorld {
    pub(super) fn vassal_job(
        &mut self,
        worker: EntityId,
        current: Option<ResourceKind>,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> Option<ResourceKind> {
        let unit = self.unit(worker)?;
        let priority = self.vassal_priority(s.player)?;
        let kinds = priority.kinds();
        if current.is_none()
            && kinds[0] == ResourceKind::Food
            && unit
                .carried
                .is_none_or(|load| load.kind == ResourceKind::Food)
            && s.avoid
                .is_none_or(|a| a.avoid_kind != Some(ResourceKind::Food))
            && self.vassal_reclaim_farm(worker, s, budget, work)
        {
            return Some(ResourceKind::Food);
        }
        if current.is_none() {
            // General construction and housing may use idle helpers. A Farm
            // foundation is considered only when Food wins resource selection.
            let p = self.player_economy(s.player)?;
            if s.complete
                && usize::from(p.population_used) + s.global_queued + 2
                    >= usize::from(p.population_cap)
                && p.population_cap < self.limits().population
                && !s
                    .buildings
                    .iter()
                    .any(|b| b.kind == BuildingKind::House && !b.is_complete())
                && self.vassal_build_near(
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
            if let Some(kind) = self.vassal_help_foundation(worker, None, s, budget, work) {
                return Some(kind);
            }
        }
        for kind in kinds {
            if unit.carried.is_some_and(|c| c.kind != kind)
                || current.is_some_and(|old| !priority.improves(kind, old))
                || s.avoid.is_some_and(|a| a.avoid_kind == Some(kind))
            {
                continue;
            }
            // Reuse paid plots before any Food foundation, but never force
            // Food ahead of the kingdom's more urgent available resource.
            if kind == ResourceKind::Food {
                if self.vassal_reclaim_farm(worker, s, budget, work) {
                    return Some(kind);
                }
                if self
                    .vassal_help_foundation(worker, Some(BuildingKind::Farm), s, budget, work)
                    .is_some()
                {
                    return Some(kind);
                }
            }
            if let Some(node) = self.vassal_resource(worker, kind, s, budget, work) {
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
                if !close && self.vassal_build_near(worker, camp, node.tile, s, budget, work) {
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
                    self.vassal_resource_result(worker, kind, node.id, accepted, s);
                    if accepted {
                        return Some(kind);
                    }
                }
            } else if kind == ResourceKind::Food && self.vassal_food(worker, s, budget, work) {
                return Some(kind);
            }
        }
        None
    }

    fn vassal_help_foundation(
        &mut self,
        worker: EntityId,
        only: Option<BuildingKind>,
        s: &Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> Option<ResourceKind> {
        let from = self.unit(worker)?.position.tile_floor();
        let attended: BTreeSet<_> = s
            .units
            .iter()
            .filter_map(|u| self.economy.builders.get(&u.id).map(|t| t.building))
            .collect();
        let mut unfinished: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| {
                !b.is_complete()
                    && only.map_or(b.kind != BuildingKind::Farm, |kind| b.kind == kind)
                    && (b.kind != BuildingKind::Farm || !attended.contains(&b.id))
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
        unfinished.sort_by_key(|b| (self.vassal_crosses_water(from, b.3), b.0, b.1));
        for (_, building, kind, _) in unfinished {
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
    pub(super) fn vassal_crosses_water(&self, from: TileCoord, to: TileCoord) -> bool {
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
