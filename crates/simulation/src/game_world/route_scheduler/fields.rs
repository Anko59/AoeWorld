//! Route guides (#978 slice 6e): a land unit waiting for a route to a chunk
//! another order of its player asked for in the last minute walks down one
//! shared guide towards that chunk, with no search slot, then its planner
//! finds the last leg to its own tile inside the chunk. A guide holds walking
//! costs over the destination chunk and the eight around it, with the
//! player's walls and gates and every building as barriers; each step of a
//! descent is checked on the live ground, and a dead end falls back to a
//! search, so a guide stays valid as trees fall and buildings rise. Its
//! build is paid from the fair route budget; it is forgotten after a minute
//! unused.
//! Attackers keep their searches: their approach decides when a fight starts.
use super::super::economy::RouteGuide;
use super::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect, WorldPosition};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// Side of a destination chunk, in tiles; a field covers it and the eight
/// around it, where most requests to it start.
pub(crate) const FIELD_CHUNK: i32 = 32;
/// Reference ticks a destination is remembered and an unused field kept.
const FIELD_TICKS: u64 = 600;

type Key = (PlayerId, (i32, i32));

#[derive(Debug, Default)]
pub(crate) struct RouteGuides {
    guides: BTreeMap<Key, (Arc<RouteGuide>, u64)>,
    asked: BTreeMap<Key, u64>,
}

impl GameWorld {
    /// Routes the waiting units a field can serve, before any search; they
    /// leave `waiting`. Field builds are charged to `budget`.
    pub(crate) fn serve_from_fields(
        &mut self,
        waiting: &mut BTreeMap<PlayerId, Vec<EntityId>>,
        budget: &mut u32,
        changed: &mut Vec<EntityId>,
    ) {
        if !matches!(self.terrain, crate::Terrain::Map { .. }) {
            return;
        }
        let (now, keep) = (self.tick.0, self.config.ticks(FIELD_TICKS));
        self.route_guides
            .guides
            .retain(|_, (_, used)| now.saturating_sub(*used) < keep);
        self.route_guides
            .asked
            .retain(|_, asked| now.saturating_sub(*asked) < keep);
        let mut built = self.prebuild_guides(waiting);
        let players: Vec<_> = waiting.keys().copied().collect();
        for player in players {
            let ids = std::mem::take(waiting.entry(player).or_default());
            let left = ids
                .into_iter()
                .filter(|&id| !self.field_route(player, id, budget, &mut built, changed))
                .collect();
            waiting.insert(player, left);
        }
        waiting.retain(|_, ids| !ids.is_empty());
    }

    /// The guide `id`'s route would walk down, or `None` when it keeps its
    /// own search: naval, attacking, mid-search, between waypoints or
    /// already in its destination chunk.
    fn field_key(&self, player: PlayerId, id: EntityId) -> Option<Key> {
        let unit = &self.units[*self.lookup.get(&id)?];
        let order = unit.order?;
        let from = unit.state.position.tile_floor();
        let target = order.target_tile;
        let chunk = |t: TileCoord| (t.x.div_euclid(FIELD_CHUNK), t.y.div_euclid(FIELD_CHUNK));
        let own_search = unit.state.kind.is_naval()
            || self.combat.attackers.contains_key(&id)
            || unit.planner.is_some()
            || order.waypoint.tile_floor() != from
            || chunk(from) == chunk(target);
        (!own_search).then_some((player, chunk(target)))
    }

    /// The tiles a guide to `key` covers: its chunk and the eight around it.
    fn guide_area(&self, key: Key) -> TileRect {
        let (cx, cy) = key.1;
        TileRect::new(
            TileCoord::new(
                ((cx - 1) * FIELD_CHUNK).max(0),
                ((cy - 1) * FIELD_CHUNK).max(0),
            ),
            TileCoord::new(
                ((cx + 2) * FIELD_CHUNK).min(self.config.width_tiles),
                ((cy + 2) * FIELD_CHUNK).min(self.config.height_tiles),
            ),
        )
    }

    /// The guides this round builds, each on its own task before any unit
    /// is routed: whether a unit builds depends only on its own order and
    /// on the marks of the units before it, and a guide reads only the
    /// ground, which routing does not change (#978).
    fn prebuild_guides(
        &self,
        waiting: &BTreeMap<PlayerId, Vec<EntityId>>,
    ) -> BTreeMap<Key, Option<RouteGuide>> {
        let guides = &self.route_guides;
        let (mut marked, mut builds) = (BTreeSet::new(), Vec::new());
        for (&player, ids) in waiting {
            for key in ids.iter().filter_map(|&id| self.field_key(player, id)) {
                let known =
                    guides.guides.contains_key(&key) || builds.iter().any(|(k, _)| *k == key);
                // The first request only marks the destination.
                if known || (!guides.asked.contains_key(&key) && marked.insert(key)) {
                    continue;
                }
                builds.push((key, None));
            }
        }
        crate::game_world::parallel::par_each_mut(&mut builds, |(key, guide)| {
            *guide = self.route_guide(key.0, guide_goal(*key), self.guide_area(*key));
        });
        builds.into_iter().collect()
    }

    fn field_route(
        &mut self,
        player: PlayerId,
        id: EntityId,
        budget: &mut u32,
        built: &mut BTreeMap<Key, Option<RouteGuide>>,
        changed: &mut Vec<EntityId>,
    ) -> bool {
        let Some(key) = self.field_key(player, id) else {
            return false;
        };
        let index = self.lookup[&id];
        let from = self.units[index].state.position.tile_floor();
        let Some(target) = self.units[index].order.map(|order| order.target_tile) else {
            return false;
        };
        let now = self.tick.0;
        let field = match self.route_guides.guides.get_mut(&key) {
            Some((field, used)) => {
                *used = now;
                field.clone()
            }
            None => {
                // The first request only marks the destination.
                if self.route_guides.asked.insert(key, now).is_none() {
                    return false;
                }
                let area = self.guide_area(key);
                // A guide reads each tile's cached step mask once.
                let cost = (area.width() * area.height()) as u32;
                // A build is never refused: it serves every later order
                // to the chunk. It spends what is left of this tick.
                *budget = budget.saturating_sub(cost);
                let built = built
                    .remove(&key)
                    .unwrap_or_else(|| self.route_guide(player, guide_goal(key), area));
                let Some(field) = built else {
                    return false;
                };
                let field = Arc::new(field);
                self.route_guides.guides.insert(key, (field.clone(), now));
                field
            }
        };
        let Some(tiles) = self.guide_descent(&field, player, from, guide_goal(key)) else {
            return false;
        };
        if tiles.len() < 2 {
            return false;
        }
        // Waypoints at the turns: a straight run of steps is one segment.
        let direction = |a: TileCoord, b: TileCoord| (b.x - a.x, b.y - a.y);
        let route = (1..tiles.len())
            .filter(|&i| {
                i + 1 == tiles.len()
                    || direction(tiles[i - 1], tiles[i]) != direction(tiles[i], tiles[i + 1])
            })
            .filter_map(|i| WorldPosition::from_tile_center(tiles[i]).ok())
            .collect::<VecDeque<_>>();
        let end = tiles[tiles.len() - 1];
        self.units[index].route = route;
        self.units[index].fast_route_tried = false;
        if end != target {
            self.store_planner(index, Some(super::leg_planner(end, target)));
        }
        changed.push(id);
        true
    }
}

/// The destination chunk of a guide to `key`.
fn guide_goal(key: Key) -> TileRect {
    let (cx, cy) = key.1;
    TileRect::from_xywh(cx * FIELD_CHUNK, cy * FIELD_CHUNK, FIELD_CHUNK, FIELD_CHUNK)
}

impl GameWorld {
    /// Steepest descent down `guide` from `from` into `goal`, each step one
    /// the planner may take on the live ground; `None` at a dead end (a
    /// house or other ground the guide does not hold), where a search takes over.
    pub(crate) fn guide_descent(
        &self,
        guide: &RouteGuide,
        player: PlayerId,
        from: TileCoord,
        goal: TileRect,
    ) -> Option<Vec<TileCoord>> {
        const OFFSETS: [(i32, i32); 8] = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];
        let mut at = from;
        let mut cost = guide.cost(from)?;
        let mut tiles = vec![from];
        // A guide's costs fall at every step, so a descent ends within its box.
        let clear = self.route_edges(player)?;
        while !goal.contains(at) {
            // Cheapest first; the live edge is read only until one is clear.
            let mut candidates: Vec<(u16, usize)> = OFFSETS
                .iter()
                .enumerate()
                .filter_map(|(i, o)| {
                    let left = guide.cost(TileCoord::new(at.x + o.0, at.y + o.1))?;
                    (left < cost).then_some((left, i))
                })
                .collect();
            candidates.sort_unstable();
            let (left, i) = candidates
                .into_iter()
                .find(|&(_, i)| clear(at, OFFSETS[i]))?;
            let next = TileCoord::new(at.x + OFFSETS[i].0, at.y + OFFSETS[i].1);
            tiles.push(next);
            (at, cost) = (next, left);
        }
        Some(tiles)
    }
}
