use super::*;
use crate::game_world::economy::gathering::ResourceSearchWork;
use aoe_map::ResourceKind;

impl GameWorld {
    pub(super) fn steward_fleet_jobs(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut StewardWork,
        changed: &mut Vec<EntityId>,
    ) {
        let split = s
            .home
            .navy
            .after
            .map_or(0, |after| s.boats.partition_point(|u| u.id <= after));
        let boats: Vec<_> = s.boats[split..]
            .iter()
            .chain(&s.boats[..split])
            .copied()
            .collect();
        for boat in boats {
            if self.economy.stewards.manual.contains(&boat.id)
                || self
                    .steward_controller(boat.id)
                    .is_some_and(|id| id != s.steward)
                || (!s.home.navy.ships.contains(&boat.id)
                    && self.economy.steward_homes[&s.steward].navy.ships.len()
                        >= STEWARD_RULES.max_villagers)
            {
                continue;
            }
            // Existing productive ships are adopted in place, without cancelling
            // their trip or discarding a load. Their current fish is known work.
            if let Some(task) = self.economy.gatherers.get(&boat.id)
                && task.fish.is_some()
                && WorldPosition::from_tile_center(task.tile).is_ok_and(|at| s.home.owns(at))
            {
                let node = if let Terrain::Map { generator, .. } = &self.terrain {
                    generator.resource_by_id(task.node)
                } else {
                    None
                };
                if let Some(node) = node
                    && budget.inspect(work, 1)
                {
                    self.steward_remember_fish(node, s, budget, work);
                }
                self.claim_steward_worker(s.steward, boat.id, ResourceKind::Food);
                continue;
            }
            if !self.steward_boat_idle(boat.id) {
                continue;
            }
            // No known fish and no search admission means another boat cannot
            // find a job this pass. Leave productive probes for land workers.
            if s.home.navy.fish.is_empty()
                && budget.searches >= STEWARD_RULES.resource_searches_per_domain
            {
                break;
            }
            if !budget.job() {
                break;
            }
            s.home.navy.after = Some(boat.id);
            if let Some(home) = self.economy.steward_homes.get_mut(&s.steward) {
                home.navy.after = Some(boat.id);
            }
            // Ships can discover the fish beside them immediately. The fixed
            // finder and retained offers share the domain's eight admissions.
            if budget.search(work) {
                let mut counted = ResourceSearchWork::default();
                let nodes = self.nearest_resources_matching_counted::<KNOWN_FISH>(
                    boat.position.tile_floor(),
                    ResourceKind::Food,
                    10,
                    None,
                    |node| {
                        self.steward_fish_known(*node, s)
                            && self.node_has_work_slot(node.id, boat.id)
                    },
                    &mut counted,
                );
                work.resource_blocks += counted.blocks;
                budget.queried_nodes(work, counted.nodes);
                for node in nodes.into_iter().flatten() {
                    self.steward_remember_fish(node, s, budget, work);
                }
            }
            let mut fish = s.home.navy.fish.clone();
            fish.sort_by_key(|node| {
                let from = boat.position.tile_floor();
                (
                    from.x.abs_diff(node.tile.x).pow(2) + from.y.abs_diff(node.tile.y).pow(2),
                    node.id,
                )
            });
            for node in fish {
                if !budget.inspect(work, 1) {
                    break;
                }
                if !self.node_has_work_slot(node.id, boat.id) {
                    continue;
                }
                if !budget.order(work) {
                    break;
                }
                if self.issue_gather(s.player, boat.id, node.id).is_ok() {
                    self.claim_steward_worker(s.steward, boat.id, ResourceKind::Food);
                    changed.push(boat.id);
                    break;
                }
            }
        }
    }
}
