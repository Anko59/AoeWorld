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
        // Reuse paid-for plots before sending another farmer to a foundation.
        if unit.carried.is_none_or(|c| c.kind == ResourceKind::Food)
            && s.avoid
                .is_none_or(|a| a.avoid_kind != Some(ResourceKind::Food))
            && self.vassal_reclaim_farm(worker, s, budget, work)
        {
            return Some(ResourceKind::Food);
        }
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
                    super::super::dropoff::ring_distance(b.footprint, unit.position.tile_floor()),
                    b.id,
                    b.kind,
                    b.footprint.min,
                )
            })
            .collect();
        unfinished.sort_unstable_by_key(|b| (b.0, b.1));
        unfinished.truncate(8);
        unfinished.sort_by_key(|b| {
            (
                self.vassal_crosses_water(unit.position.tile_floor(), b.3),
                b.0,
                b.1,
            )
        });
        for (_, building, kind, _) in unfinished {
            if budget.order(work) && self.issue_construct(s.player, &[worker], building).is_ok() {
                return Some(match kind {
                    BuildingKind::Mill | BuildingKind::Farm => ResourceKind::Food,
                    BuildingKind::MiningCamp => ResourceKind::Gold,
                    _ => ResourceKind::Wood,
                });
            }
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
        let mut tried_farms = false;
        for kind in kinds {
            if unit.carried.is_some_and(|c| c.kind != kind)
                || s.avoid.is_some_and(|a| a.avoid_kind == Some(kind))
            {
                continue;
            }
            if !budget.search(work) {
                break;
            }
            let from = unit.position.tile_floor();
            let eligible = |n: &aoe_map::ResourceNode| {
                n.object != ObjectKind::DeepFish
                    && s.home.owns(node_position(n.tile))
                    && s.avoid.is_none_or(|a| a.avoid_tile != Some(n.tile))
                    && self.visibility(s.player, n.tile) != TileVisibility::Unexplored
            };
            let node = self
                .nearest_resource_matching(from, kind, 10, None, |n| {
                    eligible(n)
                        && !self.tree_is_crowded(n, None)
                        && !self.vassal_crosses_water(from, n.tile)
                })
                .or_else(|| {
                    if budget.search(work) {
                        self.nearest_resource_matching(from, kind, 10, None, |n| {
                            eligible(n) && !self.vassal_crosses_water(from, n.tile)
                        })
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if budget.search(work) {
                        self.nearest_resource_matching(from, kind, 10, None, eligible)
                    } else {
                        None
                    }
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
            } else if kind == ResourceKind::Food {
                tried_farms = true;
                if self.vassal_food(worker, s, budget, work) {
                    return Some(kind);
                }
            }
        }
        // Empty natural-resource searches must not prevent the remaining
        // placement/order budget from putting a worker on farmland.
        if !tried_farms
            && unit.carried.is_none_or(|c| c.kind == ResourceKind::Food)
            && s.avoid
                .is_none_or(|a| a.avoid_kind != Some(ResourceKind::Food))
            && self.vassal_food(worker, s, budget, work)
        {
            return Some(ResourceKind::Food);
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
fn node_position(tile: TileCoord) -> WorldPosition {
    WorldPosition::from_tile_center(tile).unwrap_or(WorldPosition::new(0, 0))
}
