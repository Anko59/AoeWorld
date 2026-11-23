//! An unwatched group with nothing to fight rides its path (#978 4.3, slice
//! 4c): it steps on a longer stride, and on its turn every member is set on
//! its slot along the group's path instead of steering towards it. Attack-
//! movers, packed files, a group still forming and the end of the path keep
//! the full model; a slot that terrain or a building refuses ends the ride.
use super::*;
use crate::game_world::formation::layout::Rotation;

/// Reference ticks between two placements of a riding group.
pub(crate) const GROUP_RIDE_TICKS: u64 = 16;
/// Path left before the end that is always stepped, so arrival is unchanged.
const LAST_STRETCH: i64 = 24 * 1024;

impl GameWorld {
    /// Whether the group led by `leader` rides: every member it steers is an
    /// unseen bystander and the group is formed, in open order and far from
    /// its end.
    pub(crate) fn group_rides(&self, leader: EntityId, group: &Group) -> bool {
        !self.vision.human_seats.is_empty()
            && !group.settled
            && !group.attack_move
            && group.breach.is_none()
            && group.packing_files >= group.members.len()
            && group.progress - group.form_start >= group.form_distance
            && group.path.end() - group.progress > LAST_STRETCH + i64::from(group.half_width)
            && self
                .lookup
                .get(&leader)
                .is_some_and(|&i| self.units[i].order.is_some() && !self.units[i].state.planning)
            && group.members.iter().all(|m| {
                !m.detached
                    && self
                        .lookup
                        .get(&m.id)
                        .is_some_and(|&i| self.unseen_bystander(i))
            })
    }

    /// Sets every member of `leader`'s riding group on its slot. False, with
    /// nothing moved, when a slot is refused: the group then steers.
    pub(crate) fn place_riding_group(
        &mut self,
        leader: EntityId,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        let Some(group) = self.formations.get(&leader) else {
            return false;
        };
        let mut moves = Vec::with_capacity(group.members.len());
        for (at, member) in group.members.iter().enumerate() {
            let Some(&index) = self.lookup.get(&member.id) else {
                return false;
            };
            let unit = self.units[index].state;
            let progress = (group.progress + i64::from(member.offset.y)).clamp(0, group.path.end());
            let mut cursor = member.cursor;
            let (point, heading) = group.path.point(progress, &mut cursor);
            let slot =
                Rotation::new(heading).oriented(point, WorldPosition::new(member.offset.x, 0));
            let (from, to) = (unit.position.tile_floor(), slot.tile_floor());
            if self.near_deck(slot)
                || !matches!(
                    self.unit_position_step_clear(unit.kind, unit.position, slot),
                    Ok(true)
                )
                || !self.building_step_clear(from, to)
            {
                return false;
            }
            moves.push((at, index, slot, progress, cursor));
        }
        let facing = group.facing;
        for (at, index, slot, progress, cursor) in moves {
            if let Some(group) = self.formations.get_mut(&leader) {
                let member = &mut group.members[at];
                member.progress = progress;
                member.cursor = cursor;
                member.target_progress = progress;
                member.target = slot;
                member.center = slot;
                member.carry = 0;
                member.blocked = 0;
                member.stalled = 0;
            }
            let before = self.units[index].state;
            let previous_tile = before.position.tile_floor();
            let stored = &mut self.units[index];
            stored.state.previous_position = before.position;
            stored.state.position = slot;
            stored.state.facing = facing;
            let after = stored.state;
            self.track_unit_step(after.id, previous_tile, &after);
            let bucket = ChunkCoord::from_position(slot);
            if self.units[index].bucket != bucket {
                self.move_bucket(index, bucket);
            }
            if previous_tile != slot.tile_floor() {
                self.update_unit_vision(after.id);
            }
            changed.push(after.id);
        }
        true
    }
}
