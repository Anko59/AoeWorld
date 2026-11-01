//! The planner's tile route hugs walls and follows greedy partial legs, so a
//! block marching along it squeezes into single file beside every bank. The
//! anchor instead follows a clearance-seeking any-angle corridor over tiles
//! proven by the same crossing predicates.
use super::*;

/// Corridor sample spacing: consecutive samples stay in neighbouring tiles.
const SAMPLE: i64 = 512;
/// Longest straightened corridor edge, bounding one edge check.
const LONGEST_EDGE: i64 = 48 * 1024;
/// Widest lateral search for a vertex's clearer position, in tiles.
const PUSH_TILES: i32 = 7;
/// Static clearance (half tiles) that a shaped corridor seeks and keeps.
const ROOMY: u8 = 11;

impl GameWorld {
    /// Shape the known route once per change of its end: move each vertex
    /// sideways to the nearest roomy tile within seven tiles, then keep the
    /// farthest vertex whose straight edge stays at least as far from static
    /// obstacles as the vertices it replaces. The last point stays exact,
    /// because the planner continues from it. At most 4,096 + 128 per member
    /// static lookups per change; None keeps the route.
    pub(super) fn taut_route(
        &self,
        index: usize,
        group: &mut Group,
        order: MovementOrder,
    ) -> Option<Vec<WorldPosition>> {
        let route = &self.units[index].route;
        let end = route.back().copied().unwrap_or(order.waypoint);
        let shaped = group.taut_end.replace(end);
        if shaped == Some(end) || route.is_empty() || group.direct {
            return None;
        }
        let kind = self.units[index].state.kind;
        let original = std::iter::once(order.waypoint)
            .chain(route.iter().copied())
            .collect::<Vec<_>>();
        let mut budget = 4096 + 128 * group.members.len() as i64;
        let (pushed, roomy) = self.roomy_vertices(kind, group.anchor, &original, &mut budget);
        let points = self.pull_taut(kind, group.anchor, &original, &pushed, &roomy, &mut budget)?;
        (points != original).then_some(points)
    }

    fn clearance_lookup(&self, kind: UnitKind, at: WorldPosition, budget: &mut i64) -> u8 {
        *budget -= 1;
        self.group_work.collision(1);
        self.static_clearance(kind, at.tile_floor())
    }

    /// Each vertex below the roomy clearance moves perpendicular to its
    /// neighbours' chord, through crossable tiles, to the nearest tile with
    /// the most clearance (capped at roomy). The final vertex never moves.
    fn roomy_vertices(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        original: &[WorldPosition],
        budget: &mut i64,
    ) -> (Vec<WorldPosition>, Vec<u8>) {
        let mut pushed = Vec::with_capacity(original.len());
        let mut roomy = Vec::with_capacity(original.len());
        for (k, &v) in original.iter().enumerate() {
            let clearance = self.clearance_lookup(kind, v, budget);
            let mut best = (clearance.min(ROOMY), std::cmp::Reverse(0), v);
            let previous = if k == 0 { from } else { original[k - 1] };
            if let Some(&next) = original.get(k + 1)
                && clearance < ROOMY
                && *budget > 0
            {
                let (dx, dy) = (
                    (next.x - previous.x).signum(),
                    (next.y - previous.y).signum(),
                );
                for (sx, sy) in [(-dy, dx), (dy, -dx)] {
                    let mut tile = v.tile_floor();
                    for s in 1..=PUSH_TILES {
                        if (sx, sy) == (0, 0) {
                            break;
                        }
                        let to = TileCoord::new(tile.x + sx, tile.y + sy);
                        let Ok(point) = WorldPosition::from_tile_center(to) else {
                            break;
                        };
                        if !self.unit_step_clear(kind, tile, to).unwrap_or(false)
                            || !self.building_step_clear(tile, to)
                        {
                            break;
                        }
                        let value = self.clearance_lookup(kind, point, budget).min(ROOMY);
                        best = best.max((value, std::cmp::Reverse(s), point));
                        tile = to;
                        if value >= ROOMY {
                            break;
                        }
                    }
                }
            }
            pushed.push(best.2);
            roomy.push(best.0);
        }
        (pushed, roomy)
    }

    /// Greedy any-angle pull over the moved vertices: an exponential then
    /// binary search finds the farthest visible one. When none is visible the
    /// original vertex continues, and a dead end keeps the original route.
    fn pull_taut(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        original: &[WorldPosition],
        pushed: &[WorldPosition],
        roomy: &[u8],
        budget: &mut i64,
    ) -> Option<Vec<WorldPosition>> {
        let n = original.len();
        let mut out = Vec::new();
        let (mut at, mut at_roomy, mut i) = (from, self.clearance_lookup(kind, from, budget), 0);
        while i < n {
            let visible = |j: usize, budget: &mut i64| {
                let floor = roomy[i..=j].iter().copied().fold(at_roomy, u8::min);
                self.taut_edge(kind, at, pushed[j], floor.max(1), budget)
            };
            let (mut good, mut bad, mut probe) = (None, n, 0);
            while i + probe < n {
                if visible(i + probe, budget) {
                    good = Some(i + probe);
                    probe = probe * 2 + 1;
                } else {
                    bad = i + probe;
                    break;
                }
            }
            if let Some(mut lo) = good {
                while bad - lo > 1 {
                    let mid = (lo + bad) / 2;
                    if visible(mid, budget) {
                        lo = mid;
                    } else {
                        bad = mid;
                    }
                }
                out.push(pushed[lo]);
                (at, at_roomy, i) = (pushed[lo], roomy[lo], lo + 1);
            } else if self.taut_edge(kind, at, original[i], 0, budget) {
                out.push(original[i]);
                let clearance = self.clearance_lookup(kind, original[i], budget);
                (at, at_roomy, i) = (original[i], clearance.min(ROOMY), i + 1);
            } else {
                return None;
            }
        }
        (out.last() == original.last()).then_some(out)
    }

    /// A straight anchor edge: every sampled tile step is crossable, and every
    /// sample outside the end tiles keeps at least `floor` static clearance.
    fn taut_edge(
        &self,
        kind: UnitKind,
        from: WorldPosition,
        to: WorldPosition,
        floor: u8,
        budget: &mut i64,
    ) -> bool {
        let length = path::distance(from, to);
        let steps = ((length + SAMPLE - 1) / SAMPLE).max(1);
        if length > LONGEST_EDGE || *budget < steps {
            return false;
        }
        let (first, last) = (from.tile_floor(), to.tile_floor());
        let mut previous = first;
        for k in 1..=steps {
            *budget -= 1;
            self.group_work.collision(1);
            let tile = WorldPosition::new(
                from.x + (i64::from(to.x - from.x) * k / steps) as i32,
                from.y + (i64::from(to.y - from.y) * k / steps) as i32,
            )
            .tile_floor();
            if tile != previous {
                if !self.unit_step_clear(kind, previous, tile).unwrap_or(false)
                    || !self.building_step_clear(previous, tile)
                {
                    return false;
                }
                previous = tile;
            }
            if tile != first && tile != last && self.static_clearance(kind, tile) < floor {
                return false;
            }
        }
        true
    }
}
