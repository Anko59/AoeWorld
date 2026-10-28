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
    /// Ticks since the anchor arrived that this member neither reached its
    /// slot nor moved. At `GIVE_UP_AFTER_TICKS` it stops where it stands, so
    /// one boxed-in unit cannot keep the whole group unsettled.
    stalled: u16,
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

    /// An attack-move contact releases the entire formation. Orders, stops and
    /// casualties release only the units they touch: see `leave_group`.
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
            self.stop_member(index);
        }
    }

    /// One unit leaves its group: a death, a stop, a single-unit move or
    /// retaliation. The others keep route, slots, queue and pace.
    pub(crate) fn leave_group(&mut self, id: EntityId) {
        self.leave_groups(&[id]);
    }

    /// Several units leave their groups at once, for example the subset that a
    /// new order takes. Each affected group is rebuilt once; an emptied group
    /// is deleted. Every leaver stops where it stands.
    pub(crate) fn leave_groups(&mut self, ids: &[EntityId]) {
        let mut leaving = BTreeMap::<EntityId, Vec<EntityId>>::new();
        for &id in ids {
            if let Some(leader) = self.lookup.get(&id).and_then(|&i| self.units[i].formation) {
                leaving.entry(leader).or_default().push(id);
            }
        }
        let mut stopped = Vec::new();
        for (leader, mut leavers) in leaving {
            leavers.sort_unstable();
            if let Some(mut group) = self.formations.remove(&leader) {
                group
                    .members
                    .retain(|m| leavers.binary_search(&m.id).is_err());
                // Members stay in id order, so the first is the new carrier.
                if let Some(carrier) = group.members.first().map(|m| m.id) {
                    group.forget_departures();
                    if carrier != leader {
                        self.hand_over_route(leader, carrier);
                        for member in &group.members {
                            if let Some(&index) = self.lookup.get(&member.id) {
                                self.units[index].formation = Some(carrier);
                            }
                        }
                    }
                    self.formations.insert(carrier, group);
                }
            }
            stopped.extend(leavers);
        }
        for id in stopped {
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].formation = None;
                self.stop_member(index);
            }
        }
    }

    /// The group's scheduler state lives on its first member. When that unit
    /// leaves, the next one takes over the route, its planner and its flags.
    fn hand_over_route(&mut self, from: EntityId, to: EntityId) {
        let (from, at) = (self.lookup[&from], self.lookup[&to]);
        let planner = self.take_planner(from);
        let old = &mut self.units[from];
        let (order, route) = (old.order.take(), std::mem::take(&mut old.route));
        let (planning, since, fast) =
            (old.state.planning, old.planning_since, old.fast_route_tried);
        let new = &mut self.units[at];
        new.order = order;
        new.route = route;
        new.state.planning = planning;
        new.planning_since = since;
        new.fast_route_tried = fast;
        self.store_planner(at, planner);
        if order.is_some()
            && let Err(slot) = self.active_movers.binary_search(&to)
        {
            self.active_movers.insert(slot, to);
        }
    }

    /// What every released member gets: no route, no queue, no task.
    fn stop_member(&mut self, index: usize) {
        let id = self.units[index].state.id;
        self.stop_movement(index);
        self.move_queues.remove(&id);
        self.units[index].state.task = UnitTask::Idle;
        if let Ok(at) = self.active_movers.binary_search(&id) {
            self.active_movers.remove(at);
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
