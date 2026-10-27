use super::{VassalWork, rules::Budget, snapshot::Snapshot};
use crate::{BuildingKind, GameWorld, TileVisibility};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_map::{ObjectKind, ResourceKind};
use std::collections::BTreeSet;

impl GameWorld {
    pub(super) fn vassal_job(
        &mut self,
        worker: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> Option<ResourceKind> {
        let unit = self.unit(worker)?;
        let unfinished = s
            .buildings
            .iter()
            .find(|b| {
                !b.is_complete()
                    && matches!(
                        b.kind,
                        BuildingKind::House
                            | BuildingKind::LumberCamp
                            | BuildingKind::MiningCamp
                            | BuildingKind::Mill
                            | BuildingKind::Farm
                    )
            })
            .map(|b| (b.id, b.kind));
        if let Some((building, kind)) = unfinished
            && budget.order(work)
            && self.issue_construct(s.player, &[worker], building).is_ok()
        {
            return Some(match kind {
                BuildingKind::Mill | BuildingKind::Farm => ResourceKind::Food,
                BuildingKind::MiningCamp => ResourceKind::Gold,
                _ => ResourceKind::Wood,
            });
        }
        let stock = self.player_economy(s.player)?.stockpile;
        let pending_house = s
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::House && !b.is_complete());
        let p = self.player_economy(s.player)?;
        if s.complete
            && usize::from(p.population_used) + s.global_queued + 2 >= usize::from(p.population_cap)
            && p.population_cap < self.limits().population
            && !pending_house
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
        // Food/wood/gold/stone shares, with a bounded stockpile correction.
        // Prefer the carried type so reassignment never discards a useful load.
        let amounts = [stock.food, stock.wood, stock.gold, stock.stone];
        let weights = [45, 30, 15, 10];
        let mut kinds = [
            ResourceKind::Food,
            ResourceKind::Wood,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ];
        kinds.sort_by_key(|k| {
            let at = *k as usize;
            (
                unit.carried.is_some_and(|c| c.kind != *k),
                (s.allocated[at] + 1) * 100 / weights[at] + amounts[at].min(4_000) as usize / 500,
                at,
            )
        });
        for kind in kinds {
            if unit.carried.is_some_and(|c| c.kind != kind) {
                continue;
            }
            if !budget.search(work) {
                return None;
            }
            let node =
                self.nearest_resource_matching(unit.position.tile_floor(), kind, 10, None, |n| {
                    n.object != ObjectKind::DeepFish
                        && s.home.contains(node_position(n.tile))
                        && self.visibility(s.player, n.tile) != TileVisibility::Unexplored
                });
            if let Some(node) = node {
                let dropoff = s.buildings.iter().any(|b| {
                    b.kind.accepts(kind)
                        && super::super::dropoff::ring_distance(b.footprint, node.tile) <= 64
                });
                let camp = match kind {
                    ResourceKind::Food => BuildingKind::Mill,
                    ResourceKind::Wood => BuildingKind::LumberCamp,
                    ResourceKind::Gold | ResourceKind::Stone => BuildingKind::MiningCamp,
                };
                if !dropoff && self.vassal_build_near(worker, camp, node.tile, s, budget, work) {
                    return Some(kind);
                }
                if budget.order(work) && self.issue_gather(s.player, worker, node.id).is_ok() {
                    return Some(kind);
                }
            } else if kind == ResourceKind::Food && self.vassal_food(worker, s, budget, work) {
                return Some(kind);
            }
        }
        None
    }
    fn vassal_food(
        &mut self,
        worker: EntityId,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        let worked: BTreeSet<_> = s
            .units
            .iter()
            .filter_map(|u| self.economy.gatherers.get(&u.id).and_then(|t| t.farm))
            .collect();
        let farms: Vec<_> = s
            .buildings
            .iter()
            .filter(|b| {
                b.kind == BuildingKind::Farm
                    && b.is_complete()
                    && b.food_remaining.is_some_and(|n| n > 0)
                    && !worked.contains(&b.id)
            })
            .map(|b| b.id)
            .collect();
        for farm in farms.into_iter().take(1) {
            if budget.order(work) && self.issue_farm(s.player, worker, farm).is_ok() {
                return true;
            }
        }
        let Some(unit) = self.unit(worker) else {
            return false;
        };
        if !s.buildings.iter().any(|b| b.kind == BuildingKind::Mill) {
            return self.vassal_build_near(
                worker,
                BuildingKind::Mill,
                unit.position.tile_floor(),
                s,
                budget,
                work,
            );
        }
        self.vassal_build_near(
            worker,
            BuildingKind::Farm,
            unit.position.tile_floor(),
            s,
            budget,
            work,
        )
    }
}
fn node_position(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap_or(WorldPosition::new(0, 0))
}
