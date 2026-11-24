//! With a human seat, a mover no human client shows at unit zoom steps on
//! every `STRIDE`-th tick only and banks each skipped tick's movement in its
//! carry, so it covers the same ground in fewer, longer steps. Fighters and
//! anything on a screen (`visibility/views.rs`) step every tick.
//! A formation strides on its leader's phase, so an unseen group steps as one
//! block, and its targets are prepared, on one tick in four.
use super::*;

/// Reference ticks between two steps of an unseen mover (`WorldConfig::ticks`).
pub(crate) const STRIDE: u64 = 4;
/// Reference ticks between two steps of an unseen group whose leader waits
/// for its route: it only settles into its slots around a still anchor. On
/// the owner's Anatolia 4 (99 AIs) about 70% of moving groups wait, and half
/// of the group steering was spent on them (#978).
pub(crate) const WAITING_STRIDE: u64 = 16;

impl GameWorld {
    /// Removes the movers that sit out this tick, or ride (`ride.rs`), and
    /// returns them; riders moved this tick go to `changed`.
    pub(crate) fn park_unseen_movers(
        &mut self,
        movers: &mut Vec<EntityId>,
        changed: &mut Vec<EntityId>,
    ) -> Vec<EntityId> {
        let mut parked = Vec::new();
        if self.vision.human_seats.is_empty() {
            return parked;
        }
        let mut chased = std::mem::take(&mut self.combat.chased);
        chased.clear();
        chased.extend(self.combat.attackers.values().map(|a| a.target));
        chased.sort_unstable();
        chased.dedup();
        self.combat.chased = chased;
        movers.retain(|&id| {
            let rides = self
                .lookup
                .get(&id)
                .copied()
                .is_some_and(|index| self.ride(index, changed));
            let park = rides || self.park_unseen(id, false);
            if park {
                parked.push(id);
            }
            !park
        });
        parked
    }

    /// On a human client's screen at unit zoom, or near it (`views.rs`).
    fn seen_by_a_person(&self, index: usize) -> bool {
        let unit = &self.units[index].state;
        self.watched_tile(unit.position.tile_floor()) && !self.embarked(unit.id)
    }

    /// Whether the unit at `index` sits out this tick: it is an unseen
    /// bystander and its turn (its leader's, in a formation) is not due.
    fn sits_out(&self, index: usize) -> bool {
        let id = self.units[index].state.id;
        let phase = self.units[index].formation.unwrap_or(id);
        !self.vision.human_seats.is_empty()
            && !(self.tick.0 + u64::from(phase.0))
                .is_multiple_of(self.config.ticks(self.stride_of(phase)))
            && self.unseen_bystander(index)
    }

    /// The stride of the unit or group led by `phase`: longer for a group
    /// whose leader waits for its route.
    pub(super) fn stride_of(&self, phase: EntityId) -> u64 {
        let waits = self.formations.contains_key(&phase)
            && self
                .lookup
                .get(&phase)
                .is_some_and(|&index| self.waiting_for_route(index));
        if waits { WAITING_STRIDE } else { STRIDE }
    }

    /// No person sees the unit at `index` and it is not fighting, attacking
    /// or attacked. Fighters keep the full model: an attacker that moves in
    /// jumps strikes less often (the first stride experiment lost 5-7% of
    /// kills per tick), and so does one chasing a target that moves in jumps.
    pub(crate) fn unseen_bystander(&self, index: usize) -> bool {
        let id = self.units[index].state.id;
        !self.vision.human_seats.is_empty()
            && !self.combat.attackers.contains_key(&id)
            && self.units[index].state.attack_target.is_none()
            && self.combat.chased.binary_search(&id).is_err()
            && !self.seen_by_a_person(index)
    }

    /// Whether every member the group steers sits out this tick; its targets
    /// then stay as they are, since its progress cannot move. Detached
    /// members fight or walk on their own and never read those targets.
    pub(crate) fn group_sits_out(&self, leader: EntityId, group: &Group) -> bool {
        !self.vision.human_seats.is_empty()
            && !(self.tick.0 + u64::from(leader.0))
                .is_multiple_of(self.config.ticks(self.stride_of(leader)))
            && group.members.iter().filter(|m| !m.detached).all(|m| {
                self.lookup
                    .get(&m.id)
                    .is_some_and(|&index| self.sits_out(index))
            })
    }

    /// True when `id` sits out this tick (see `sits_out`). Its skipped
    /// movement is banked: `as_member` when its group's steering parks it,
    /// else the movers' loop does (see `bank_rest`).
    pub(crate) fn park_unseen(&mut self, id: EntityId, as_member: bool) -> bool {
        if self.vision.human_seats.is_empty() {
            return false;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        self.sits_out(index) && self.bank_rest(index, id, as_member)
    }

    /// `bank_rest` for every steered member of the resting group led by
    /// `leader`: one group lookup, and a member's credit computed once per
    /// kind, owner and road state.
    pub(crate) fn bank_group_rest(&mut self, leader: EntityId) {
        let Some(group) = self.formations.get(&leader) else {
            return;
        };
        // A group waiting for its route holds a still anchor: its members'
        // slots do not move, so their skipped walk is not banked.
        if self.stride_of(leader) == WAITING_STRIDE {
            return;
        }
        let pace = group.pace;
        let mut credits = Vec::<((UnitKind, aoe_core::PlayerId, bool), u64)>::new();
        let mut members = Vec::with_capacity(group.members.len());
        let mut others = Vec::new();
        for (at, member) in group
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.detached)
        {
            let Some(&index) = self.lookup.get(&member.id) else {
                continue;
            };
            // A leader with an order of its own carries the group and banks
            // against that order; one without walks a slot like the rest.
            let carrier = member.id == leader && self.units[index].order.is_some();
            if carrier || self.units[index].formation != Some(leader) {
                others.push((index, member.id));
                continue;
            }
            let unit = &self.units[index].state;
            let on_road = !self.economy.roads.is_empty()
                && !unit.kind.is_naval()
                && self.economy.roads.contains(&unit.position.tile_floor());
            let key = (unit.kind, unit.player, on_road);
            let credit = match credits.iter().find(|(k, _)| *k == key) {
                Some(&(_, credit)) => credit,
                None => {
                    let own = self.movement_speed(unit.kind, unit.player);
                    let road = self.road_speed(unit.kind, unit.position, own);
                    let credit = super::super::matched_speed(pace, road);
                    credits.push((key, credit));
                    credit
                }
            };
            members.push((at, credit));
        }
        if let Some(group) = self.formations.get_mut(&leader) {
            for (at, credit) in members {
                group.members[at].carry = group.members[at].carry.saturating_add(credit);
            }
        }
        for (index, id) in others {
            self.bank_rest(index, id, true);
        }
    }

    /// Banks the slot walk the member `id` of `leader`'s group skips.
    fn bank_slot_walk(&mut self, index: usize, id: EntityId, leader: EntityId) -> bool {
        let unit = self.units[index].state;
        let own = self.movement_speed(unit.kind, unit.player);
        let road = self.road_speed(unit.kind, unit.position, own);
        let Some(group) = self.formations.get_mut(&leader) else {
            return false;
        };
        let Some(member) = group.member_index(id) else {
            return false;
        };
        let credit = super::super::matched_speed(group.pace, road);
        group.members[member].carry = group.members[member].carry.saturating_add(credit);
        true
    }

    /// Banks the movement the unit at `index` skips this tick.
    ///
    /// A leader carrying its group on its order sits in two lists: the
    /// movers' loop, where its order moves the anchor, and its group's
    /// steering, where it walks its own slot. Each banks its own part:
    /// `as_member` the slot walk, else the anchor. Banking the anchor in both
    /// moved a strided group 1.5 times as fast as its pace (parity test 3:
    /// first contact 12 s sooner, knights ahead of their archers); banking
    /// no slot walk left the leader trailing at a quarter pace (test 4).
    pub(crate) fn bank_rest(&mut self, index: usize, id: EntityId, as_member: bool) -> bool {
        let unit = self.units[index].state;
        // A formation member has no order of its own: it banks the credit
        // `formation_motion` would give it this tick. So does a leader that
        // walks its own slot without an order.
        if let Some(leader) = self.units[index].formation
            && (leader != id || self.units[index].order.is_none() || as_member)
        {
            return self.bank_slot_walk(index, id, leader);
        }
        if self.units[index].order.is_none() {
            return false;
        }
        let banked = match self.formations.get(&id) {
            Some(group) => group.pace.0,
            None => {
                self.road_speed(
                    unit.kind,
                    unit.position,
                    self.movement_speed(unit.kind, unit.player),
                )
                .0
            }
        };
        self.units.bank_speed_carry(index, banked)
    }
}

#[path = "stride/tests.rs"]
#[cfg(test)]
mod tests;
