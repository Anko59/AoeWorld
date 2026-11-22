//! With a human seat, a mover no human client shows at unit zoom steps on
//! every `STRIDE`-th tick only and banks each skipped tick's movement in its
//! carry, so it covers the same ground in fewer, longer steps. Fighters and
//! anything on a screen (`visibility/views.rs`) step every tick.
//! A formation strides on its leader's phase, so an unseen group steps as one
//! block, and its targets are prepared, on one tick in four.
use super::*;

/// Reference ticks between two steps of an unseen mover (`WorldConfig::ticks`).
pub(crate) const STRIDE: u64 = 4;

impl GameWorld {
    /// Removes the movers that sit out this tick and returns them.
    pub(crate) fn park_unseen_movers(&mut self, movers: &mut Vec<EntityId>) -> Vec<EntityId> {
        let mut parked = Vec::new();
        if self.vision.human_seats.is_empty() {
            return parked;
        }
        movers.retain(|&id| {
            let park = self.park_unseen(id);
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
            && !(self.tick.0 + u64::from(phase.0)).is_multiple_of(self.config.ticks(STRIDE))
            && self.unseen_bystander(index)
    }

    /// No person sees the unit at `index` and it is not fighting. Fighters
    /// keep the full model: an attacker that moves in jumps strikes less often
    /// (the first stride experiment lost 5-7% of kills per tick). So does an
    /// attack-mover with something to fight near (#978 4.3, "a target"), or
    /// fights would depend on who looks (parity tests 3 and 4).
    pub(crate) fn unseen_bystander(&self, index: usize) -> bool {
        !self.vision.human_seats.is_empty()
            && !self
                .combat
                .attackers
                .contains_key(&self.units[index].state.id)
            && self.units[index].state.attack_target.is_none()
            && !self.seen_by_a_person(index)
            && !self.attack_mover_in_contact(index)
    }

    /// On attack-move, with a unit or building of another player in its
    /// chunk or the eight around: the simple contact radius of #978 4.3
    /// ("a target"). Off screen the stride tier changed how fights end
    /// (parity tests 3 and 7), so these keep the full model until step 5.
    /// Narrower radii (two and three rings of 8-tile cells, the own chunk)
    /// left fight 3 apart.
    fn attack_mover_in_contact(&self, index: usize) -> bool {
        let unit = &self.units[index].state;
        if self.offensive_continuation(unit.id).is_none() {
            return false;
        }
        let at = self.units[index].bucket;
        (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                let chunk = aoe_core::ChunkCoord {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                self.chunks.presence.others(chunk, unit.player)
                    || self.vision.building_chunks.others(chunk, unit.player)
            })
        })
    }

    /// Whether every member the group steers sits out this tick; its targets
    /// then stay as they are, since its progress cannot move. Detached
    /// members fight or walk on their own and never read those targets.
    pub(crate) fn group_sits_out(&self, leader: EntityId, group: &Group) -> bool {
        !self.vision.human_seats.is_empty()
            && !(self.tick.0 + u64::from(leader.0)).is_multiple_of(self.config.ticks(STRIDE))
            && group.members.iter().filter(|m| !m.detached).all(|m| {
                self.lookup
                    .get(&m.id)
                    .is_some_and(|&index| self.sits_out(index))
            })
    }

    /// True when `id` sits out this tick (see `sits_out`). Its skipped
    /// movement is banked.
    pub(crate) fn park_unseen(&mut self, id: EntityId) -> bool {
        if self.vision.human_seats.is_empty() {
            return false;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        self.sits_out(index) && self.bank_rest(index, id)
    }

    /// `bank_rest` for every steered member of the resting group led by
    /// `leader`: one group lookup, and a member's credit computed once per
    /// kind, owner and road state.
    pub(crate) fn bank_group_rest(&mut self, leader: EntityId) {
        let Some(group) = self.formations.get(&leader) else {
            return;
        };
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
            self.bank_rest(index, id);
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
    pub(crate) fn bank_rest(&mut self, index: usize, id: EntityId) -> bool {
        let unit = self.units[index].state;
        // A formation member has no order of its own: it banks the credit
        // `formation_motion` would give it this tick. So does a leader that
        // walks its own slot without an order.
        if let Some(leader) = self.units[index].formation
            && (leader != id || self.units[index].order.is_none())
        {
            return self.bank_slot_walk(index, id, leader);
        }
        if self.units[index].order.is_none() {
            return false;
        }
        let banked = match self.formations.get(&id) {
            Some(group) => {
                let pace = group.pace.0;
                // The leader carries its group's anchor on its order and also
                // walks its own slot: it banks both, or it trails its group
                // at a quarter of its pace (parity test 4).
                self.bank_slot_walk(index, id, id);
                pace
            }
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
