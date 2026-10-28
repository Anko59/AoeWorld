//! The first member carries the group's scheduler state, never a second unit.
//! Its physical position stays independent of the shared centroid anchor.
use super::*;
use aoe_core::{Formation, MAX_FORMATION_MEMBERS, MAX_MOVE_DESTINATIONS};

mod anchor;
mod arrival;
mod column;
mod combat;
mod hash;
pub(super) mod layout;
mod orders;
mod selection;
mod steering;
pub(crate) use steering::STEERING_WORK_PER_TICK;

#[derive(Debug)]
pub(crate) struct Member {
    id: EntityId,
    offset: WorldPosition,
    carry: u64,
    probe: usize,
    /// A tile-pair visibility result; immediate steps are always revalidated.
    cache: Option<(TileCoord, TileCoord, WorldPosition, steering::Goal)>,
    detour: VecDeque<WorldPosition>,
}

#[derive(Debug)]
pub(crate) struct Group {
    form: Formation,
    members: Vec<Member>,
    anchor: WorldPosition,
    /// Stable order orientation; tile refinement must not rotate the whole army.
    heading: WorldPosition,
    /// Current anchor path leg, used while following a column around bends.
    travel_heading: WorldPosition,
    facing: Facing,
    speed: (u64, u64),
    ready: bool,
    settled: bool,
    column: bool,
    /// Derived solely from the hashed member offsets and ids, rebuilt on orders.
    column_order: Vec<usize>,
    /// Point lookups only; member storage always remains in canonical id order.
    member_lookup: std::collections::HashMap<EntityId, usize>,
    destination: WorldPosition,
    queue: VecDeque<(WorldPosition, bool)>,
    attack_move: bool,
    /// At most 128 tile crossings, shared by every member while squeezing.
    trail: VecDeque<WorldPosition>,
}

impl GameWorld {
    pub fn formation(&self, id: EntityId) -> Option<Formation> {
        let leader = self.units[*self.lookup.get(&id)?].formation?;
        Some(self.formations.get(&leader)?.form)
    }

    pub fn formation_anchor(&self, id: EntityId) -> Option<WorldPosition> {
        let leader = self.units[*self.lookup.get(&id)?].formation?;
        Some(self.formations.get(&leader)?.anchor)
    }

    pub fn formation_slot(&self, id: EntityId) -> Option<WorldPosition> {
        let leader = self.units[*self.lookup.get(&id)?].formation?;
        let group = self.formations.get(&leader)?;
        let member = group.members.iter().find(|m| m.id == id)?;
        Some(layout::oriented(group.anchor, member.offset, group.heading))
    }

    /// Whether the group is marching single file; baseline scenarios read it.
    #[cfg(test)]
    pub(crate) fn formation_is_column(&self, id: EntityId) -> Option<bool> {
        let leader = self.units[*self.lookup.get(&id)?].formation?;
        Some(self.formations.get(&leader)?.column)
    }

    /// A replacement/combat/stop order releases the entire formation.
    pub(crate) fn break_formation(&mut self, id: EntityId) {
        let Some(leader) = self.lookup.get(&id).and_then(|&i| self.units[i].formation) else {
            return;
        };
        let Some(group) = self.formations.remove(&leader) else {
            return;
        };
        for member in group.members {
            let Some(&index) = self.lookup.get(&member.id) else {
                continue;
            };
            self.units[index].formation = None;
            self.stop_movement(index);
            self.move_queues.remove(&member.id);
            self.units[index].state.task = UnitTask::Idle;
            if let Ok(at) = self.active_movers.binary_search(&member.id) {
                self.active_movers.remove(at);
            }
        }
    }

    pub(crate) fn formation_destinations(&self, id: EntityId) -> Option<Vec<WorldPosition>> {
        let leader = self.units[*self.lookup.get(&id)?].formation?;
        let group = self.formations.get(&leader)?;
        Some(if group.settled {
            Vec::new()
        } else {
            std::iter::once(group.destination)
                .chain(group.queue.iter().map(|&(point, _)| point))
                .collect()
        })
    }
}
