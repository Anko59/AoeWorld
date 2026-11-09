//! Proofs that let the ordered movement phase skip a check. A cached clearance
//! value proves an empty square of complete static tiles. A step certificate
//! proves that no body can be in the way of a proposed step when its unit's
//! turn comes, so that turn needs no second body sweep.
use super::Tables;
use crate::GameWorld;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord, WorldPosition};

#[derive(Clone, Copy, Debug)]
pub(in crate::game_world) struct StaticArea {
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
}
impl StaticArea {
    pub(in crate::game_world) fn new(tile: TileCoord, clearance: u8) -> Option<Self> {
        if clearance == 0 {
            return None;
        }
        // c=2*d-1: every tile at Chebyshev distance <d is walkable.
        // A saturated 15 is a lower bound, so it still proves seven cells.
        let radius = i64::from((clearance - 1) / 2);
        Some(Self {
            min_x: (i64::from(tile.x) - radius) * 1024,
            min_y: (i64::from(tile.y) - radius) * 1024,
            max_x: (i64::from(tile.x) + radius + 1) * 1024,
            max_y: (i64::from(tile.y) + radius + 1) * 1024,
        })
    }
    pub(in crate::game_world) fn covers(self, from: WorldPosition, radius: i32, step: i32) -> bool {
        let reach = i64::from(radius) + i64::from(step);
        i64::from(from.x) - reach >= self.min_x
            && i64::from(from.y) - reach >= self.min_y
            && i64::from(from.x) + reach <= self.max_x
            && i64::from(from.y) + reach <= self.max_y
    }
}

impl GameWorld {
    /// Padding of an ordinary batch's step certificates: the whole speed
    /// credit of its fastest mover, no less than that mover's turn computes,
    /// plus two subunits because positions are rounded. An estimate, not the
    /// proof: a body pushed off its segment steps back farther, so the
    /// collision index measures what bodies actually do
    /// (`CollisionIndex::watch_drift`). Zero, for no certificate, when the
    /// fastest mover covers more than one tile a tick.
    pub(super) fn mover_step_bound(
        &self,
        movers: impl Iterator<Item = usize>,
        tables: &Tables,
    ) -> i32 {
        let roads = !self.economy.roads.is_empty();
        let mut bound = 0;
        for index in movers {
            let stored = &self.units[index];
            let Some(order) = stored.order else {
                continue;
            };
            if stored.formation.is_some() && !self.is_detached_member(index) {
                continue;
            }
            let unit = &stored.state;
            let (mut speed, denominator) =
                self.movement_speed_with(unit.kind, tables.economy(self, unit.player));
            // Any tile may be a road: cover the fastest step, as for groups.
            if roads && !unit.kind.is_naval() {
                speed += speed / 4;
            }
            let credit = u128::from(order.speed_carry) + u128::from(speed);
            bound = bound.max(credit / u128::from(denominator.max(1)));
        }
        // Above one tile a tick no step is certified: every padded sweep would
        // read a window several tiles wide, far more than the ordered sweep
        // it replaces.
        if bound > u128::from(FIXED_SUBUNITS_PER_TILE.unsigned_abs()) {
            0
        } else {
            bound as i32 + 2
        }
    }

    /// True when every other body's centre is at least the full radii sum plus
    /// `padding` from the whole step, in a snapshot without sampled cells.
    /// Then, wherever those bodies go within `padding` subunits each,
    /// `collision_sweep` accepts the step: none of its contact distances
    /// exceeds the radii sum and its exemptions only skip bodies. Unlike that
    /// sweep this one softens nothing and exempts nobody.
    fn step_separated(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        padding: i32,
    ) -> bool {
        if padding <= 0 || self.collisions.has_overfull_cells() {
            return false;
        }
        let unit = &self.units[index].state;
        let naval = unit.kind.is_naval();
        let own = unit.kind.collision_radius();
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let length = dx * dx + dy * dy;
        let midpoint = WorldPosition::new(
            ((i64::from(from.x) + i64::from(to.x)) / 2) as i32,
            ((i64::from(from.y) + i64::from(to.y)) / 2) as i32,
        );
        // A body that can reach the step has its centre in the step's box
        // grown by this much, and that box lies within `reach` of the midpoint.
        let broad = own + self.collisions.maximum_radii()[usize::from(naval)] + padding;
        let reach = broad + ((dx.abs().max(dy.abs()) + 1) / 2) as i32;
        let clamp = |value: i64| value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let window = (
            clamp(i64::from(from.x.min(to.x)) - i64::from(broad)),
            clamp(i64::from(from.x.max(to.x)) + i64::from(broad)),
            clamp(i64::from(from.y.min(to.y)) - i64::from(broad)),
            clamp(i64::from(from.y.max(to.y)) + i64::from(broad)),
        );
        !self.collisions.blocked_in(
            (midpoint, reach),
            window,
            naval,
            self.tick.0.wrapping_add(u64::from(unit.id.0)),
            &self.group_work,
            |id| {
                if id == unit.id {
                    return false;
                }
                let other = &self.units[self.collisions.unit_index(id)].state;
                let r = i64::from(own + other.kind.collision_radius() + padding);
                let x = i64::from(other.position.x) - i64::from(from.x);
                let y = i64::from(other.position.y) - i64::from(from.y);
                let dot = x * dx + y * dy;
                // Exact distance from the centre to the segment, against `r`.
                if dot <= 0 {
                    x * x + y * y < r * r
                } else if dot >= length {
                    let x = i64::from(other.position.x) - i64::from(to.x);
                    let y = i64::from(other.position.y) - i64::from(to.y);
                    x * x + y * y < r * r
                } else {
                    let cross = i128::from(x) * i128::from(dy) - i128::from(y) * i128::from(dx);
                    cross * cross < i128::from(r * r) * i128::from(length)
                }
            },
            |_| true,
        )
    }

    /// `local_unit_step_clear` for a parallel proposal: `None` when the step
    /// is not clear, else whether it is certified against every mover with an
    /// earlier turn (`padding` from `mover_step_bound`). A certified step is
    /// clear in the snapshot too, so its ordinary sweep is not run.
    pub(super) fn proposal_step_clear(
        &self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
        padding: i32,
    ) -> Option<bool> {
        if from == to {
            // No sweep ever refuses a step that does not move.
            return Some(true);
        }
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("avoidance_query"));
        let certified = self.step_separated(index, from, to, padding);
        ((certified || self.collision_sweep(index, from, to, false, false))
            && self.body_step_clear(self.units[index].state.kind, from, to))
        .then_some(certified)
    }

    /// Debug builds run the sweep a certificate skipped: the two must agree.
    /// The audit is not engine work, so its probes are not counted.
    #[cfg(debug_assertions)]
    pub(super) fn audit_certified_step(
        &mut self,
        index: usize,
        from: WorldPosition,
        to: WorldPosition,
    ) {
        let work = std::mem::take(&mut self.group_work);
        let clear = self.collision_sweep(index, from, to, false, false);
        self.group_work = work;
        assert!(
            clear,
            "certified step refused by the ordered sweep: unit {index} from {from:?} to {to:?}"
        );
    }
}
