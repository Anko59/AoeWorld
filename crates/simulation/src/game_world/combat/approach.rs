//! Bounded, canonical search for a firing tile in the attacker's component.
use super::{MELEE_RANGE_SUBUNITS, tick::Goal};
use crate::game_world::movement_step::{CellHasher, tile_key};
use crate::{GameWorld, GameWorldError};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap, HashMap},
    hash::BuildHasherDefault,
};

/// Bounds both search work and retained frontier entries, independent of map size.
const MAX_ATTACK_SEARCH_TILES: usize = 1_024;
/// Targets within this many tiles are checked for reachability on the region layer.
const REGION_PROOF_TILES: i64 = 400;
/// Candidate tiles proven on the region layer per search, so a target in a
/// sealed pocket costs a bounded number of proofs.
const MAX_HIERARCHY_PROOFS: u32 = 4;
const NEIGHBOURS: [(i32, i32); 8] = [
    (0, -1),
    (-1, 0),
    (1, 0),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];
/// A melee attacker that finds every spot round its target taken waits
/// only within this many tiles of its reach; farther off it walks up.
const CROWD_WAIT_TILES: i64 = 4;
/// A far firing tile is reused while the target keeps its tile and the
/// attacker is farther than this: the local search could not meet it anyway.
pub(crate) const FAR_REUSE_TILES: i64 = 64;

/// The firing tile of a far search, valid for one target tile.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FarAttack {
    pub(crate) centre: TileCoord,
    pub(crate) destination: WorldPosition,
}

/// Why a search found no firing tile yet: every free spot is taken.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SearchWait {
    Crowded,
}

#[derive(Debug)]
pub(crate) struct AttackSearch {
    pub(crate) destination: Option<WorldPosition>,
    /// The local search hit its cap and the far fallback decided.
    pub(crate) far: bool,
    /// Unit congestion deferred this search.
    pub(crate) wait: Option<SearchWait>,
    /// `destination` was chosen because its spot was free: an attacker
    /// earlier in the same loop may since have taken it. Far tiles and a
    /// crowd's preferred spot are never free by design.
    pub(crate) free_spot: bool,
}

impl GameWorld {
    /// Search outward from the attacker, prioritizing travel distance plus a
    /// lower bound on distance still needed to reach attack range. Reaching a
    /// tile proves connectivity; merely being passable near the target does not.
    pub(crate) fn attack_search(
        &mut self,
        id: EntityId,
        goal: &Goal,
    ) -> Result<AttackSearch, GameWorldError> {
        // A search the tick prefetched on the phase snapshot is used while
        // its free spot is still free; an attacker earlier in this loop may
        // have taken it, which the snapshot could not see, and then it runs
        // here. Far tiles and a crowd's preferred spot are kept as computed.
        self.combat.stats.attack_searches += 1;
        let cached = self
            .combat
            .prefetched
            .remove(&id)
            .filter(|(at, search)| {
                at == goal
                    && (!search.free_spot
                        || search
                            .destination
                            .is_none_or(|spot| self.interaction_point_free(id, spot)))
            })
            .map(|(_, search)| search);
        #[cfg(test)]
        if cached.is_some() {
            self.combat.prefetch_used += 1;
        }
        let search = match cached {
            Some(search) => search,
            None => self.attack_search_core(id, goal)?,
        };
        #[cfg(test)]
        if search.wait == Some(SearchWait::Crowded) {
            self.combat.waits += 1;
        }
        Ok(search)
    }

    /// The search itself, read-only so that several can run side by side.
    pub(super) fn attack_search_core(
        &self,
        id: EntityId,
        goal: &Goal,
    ) -> Result<AttackSearch, GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let stats = self.unit_state_stats(&unit);
        let range = super::super::navy::attack_range(&stats);
        let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
        let origin = unit.position.tile_floor();
        let mut spots =
            Self::uses_melee_position(unit.kind).then(|| self.melee_positions(&unit, range, goal));
        // Spots are nearest first. Only the first four free ones get the
        // cheap straight-line proof, so test them lazily: the rest are needed
        // only by the bounded search below.
        let mut free = Vec::new();
        let mut melee = None;
        if let Some(points) = &mut spots {
            let mut nearest = None;
            for at in points.by_ref() {
                nearest.get_or_insert(at);
                if !self.interaction_point_free(id, at) {
                    continue;
                }
                free.push(at);
                if self.interaction_line_clear(unit.kind, unit.position, at) {
                    return Ok(AttackSearch {
                        destination: Some(at),
                        far: false,
                        wait: None,
                        free_spot: true,
                    });
                }
                if free.len() == 4 {
                    break;
                }
            }
            let Some(nearest) = nearest else {
                return Ok(AttackSearch {
                    destination: None,
                    far: false,
                    wait: None,
                    free_spot: false,
                });
            };
            if free.is_empty() {
                // In a crowd the endpoint is a preference. Static route
                // admission and physical sweeps still prove every step. An
                // attacker still far off walks up and chooses again there:
                // the spots it saw taken may be free when it arrives.
                let near = range + CROWD_WAIT_TILES * MELEE_RANGE_SUBUNITS;
                if self.crowded_melee_contact(id, goal)
                    || goal.distance_squared(unit.position) > near * near
                {
                    return Ok(AttackSearch {
                        destination: Some(nearest),
                        far: false,
                        wait: None,
                        free_spot: false,
                    });
                }
                return Ok(AttackSearch {
                    destination: None,
                    far: false,
                    wait: Some(SearchWait::Crowded),
                    free_spot: false,
                });
            }
        }
        if let Some(points) = spots {
            free.extend(points.filter(|&at| self.interaction_point_free(id, at)));
            melee = Some(free);
        }
        let _span = self
            .planning_probe
            .as_ref()
            .and_then(|p| p.movement_span("attack_position_search"));
        let estimate = |at: WorldPosition| {
            // Diagonal steps cost 1448 rather than sqrt(2)*1024. This small
            // discount keeps the Euclidean lower bound below that grid cost.
            (goal.distance_squared(at) as u64)
                .isqrt()
                .saturating_sub(range as u64)
                * 999
                / 1000
        };
        // Only point reads and writes: the heap alone orders the search.
        let mut reached: HashMap<u64, u64, BuildHasherDefault<CellHasher>> =
            HashMap::with_capacity_and_hasher(64, BuildHasherDefault::default());
        reached.insert(tile_key(origin), 0);
        let mut frontier = BinaryHeap::from([Reverse((
            estimate(unit.position),
            0_u64,
            origin.y,
            origin.x,
        ))]);
        let mut expanded = 0;
        let mut capped = false;
        'search: while let Some(Reverse((_, cost, y, x))) = frontier.pop() {
            let tile = TileCoord::new(x, y);
            if reached.get(&tile_key(tile)) != Some(&cost) {
                continue;
            }
            let at = WorldPosition::from_tile_center(tile)
                .map_err(|_| GameWorldError::InvalidPosition)?;
            let distance = goal.distance_squared(at);
            if distance <= range * range && distance >= min * min {
                let destination = if let Some(points) = &melee {
                    points.iter().copied().find(|p| p.tile_floor() == tile)
                } else if self.interaction_point_free(id, at) {
                    Some(at)
                } else {
                    None
                };
                if let Some(destination) = destination {
                    return Ok(AttackSearch {
                        destination: Some(destination),
                        far: false,
                        wait: None,
                        free_spot: true,
                    });
                }
            }
            expanded += 1;
            if expanded >= MAX_ATTACK_SEARCH_TILES {
                capped = true;
                break;
            }
            for (dx, dy) in [
                (0, -1),
                (-1, 0),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(x + dx, y + dy);
                let next_cost = cost + if dx == 0 || dy == 0 { 1024 } else { 1448 };
                if reached
                    .get(&tile_key(next))
                    .is_some_and(|&old| old <= next_cost)
                    || self.economy.blocked_tiles.contains(&next)
                    || (dx != 0
                        && dy != 0
                        && [TileCoord::new(x + dx, y), TileCoord::new(x, y + dy)]
                            .iter()
                            .any(|t| self.economy.blocked_tiles.contains(t)))
                    || !self
                        .unit_step_clear(unit.kind, tile, next)
                        .map_err(GameWorldError::Environment)?
                {
                    continue;
                }
                // Count inserted entries as well as expansions, including stale
                // heap entries, so the retained memory has a fixed ceiling.
                if reached.len() >= MAX_ATTACK_SEARCH_TILES
                    || frontier.len() >= MAX_ATTACK_SEARCH_TILES
                {
                    capped = true;
                    break 'search;
                }
                let at = WorldPosition::from_tile_center(next)
                    .map_err(|_| GameWorldError::InvalidPosition)?;
                reached.insert(tile_key(next), next_cost);
                frontier.push(Reverse((
                    next_cost + estimate(at),
                    next_cost,
                    next.y,
                    next.x,
                )));
            }
        }
        if capped {
            // The bounded local search ran out before it met the target: a
            // far target. Pick the firing tile at the target instead.
            return Ok(AttackSearch {
                destination: self.far_attack_destination(
                    unit.kind,
                    unit.player,
                    origin,
                    goal,
                    range,
                    min,
                )?,
                far: true,
                wait: None,
                free_spot: false,
            });
        }
        Ok(AttackSearch {
            destination: None,
            far: false,
            wait: None,
            free_spot: false,
        })
    }

    /// Firing tile for a target beyond the local search: the nearest open tile
    /// to the goal that is in range, canonical order (distance, y, x). Within
    /// `REGION_PROOF_TILES` the attacker's region layer (its gates included)
    /// also proves that it can reach the tile; farther away the walk itself plans legs
    /// as it goes, like any far move order, and a hopeless chase ends at the
    /// next repath. Work per call is bounded by the candidate window and the
    /// abstract-node cap, never by the distance.
    fn far_attack_destination(
        &self,
        kind: crate::UnitKind,
        player: aoe_core::PlayerId,
        origin: TileCoord,
        goal: &Goal,
        range: i64,
        min: i64,
    ) -> Result<Option<WorldPosition>, GameWorldError> {
        let centre = goal.centre().tile_floor();
        let reach = i32::try_from(range / MELEE_RANGE_SUBUNITS).unwrap_or(0) + 2;
        let span = match goal {
            Goal::Unit(_, _) => 0,
            Goal::Building(rect) => rect.width().max(rect.height()),
        };
        let radius = reach + span;
        let mut candidates = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let tile = TileCoord::new(centre.x + dx, centre.y + dy);
                // The window may cross the west or north map edge, where a
                // tile has no centre; such a tile is simply no candidate.
                let Ok(at) = WorldPosition::from_tile_center(tile) else {
                    continue;
                };
                let distance = goal.distance_squared(at);
                if distance <= range * range && distance >= min * min {
                    candidates.push((distance, tile.y, tile.x, at));
                }
            }
        }
        candidates.sort_unstable_by_key(|&(distance, y, x, _)| (distance, y, x));
        let near = (i64::from(origin.x) - i64::from(centre.x))
            .abs()
            .max((i64::from(origin.y) - i64::from(centre.y)).abs())
            <= REGION_PROOF_TILES;
        let bucket = aoe_map::LandBucket::for_radius(kind.collision_radius())
            .filter(|_| near && !kind.is_naval());
        pick_firing_tile(
            candidates,
            bucket.is_some(),
            |tile| {
                Ok(!self.economy.blocked_tiles.contains(&tile)
                    && self
                        .unit_step_clear(kind, tile, tile)
                        .map_err(GameWorldError::Environment)?)
            },
            |from, to| {
                Ok(self.unit_step_clear(kind, from, to).unwrap_or(false)
                    && self.unit_step_clear(kind, to, from).unwrap_or(false))
            },
            |tile| {
                bucket.is_none_or(|bucket| {
                    !matches!(self.land_reachable(player, bucket, origin, tile), Ok(false))
                })
            },
        )
    }
}

/// The first candidate (in canonical order) that is open and, when `prove`,
/// reachable. A tile proven unreachable condemns every candidate joined to it
/// by clear steps through the candidate window without another proof, so an
/// islet with dozens of firing tiles costs one proof, not dozens. Only
/// `MAX_HIERARCHY_PROOFS` separate groups are proven per search; past that the
/// next open candidate is taken unproven, as beyond `REGION_PROOF_TILES`:
/// the walk plans its legs and a hopeless chase ends at the next repath, so
/// the cap never rejects an order that has a reachable firing tile.
fn pick_firing_tile(
    candidates: Vec<(i64, i32, i32, WorldPosition)>,
    prove: bool,
    open: impl Fn(TileCoord) -> Result<bool, GameWorldError>,
    linked: impl Fn(TileCoord, TileCoord) -> Result<bool, GameWorldError>,
    reachable: impl Fn(TileCoord) -> bool,
) -> Result<Option<WorldPosition>, GameWorldError> {
    let mut usable = BTreeSet::new();
    for &(_, y, x, _) in &candidates {
        let tile = TileCoord::new(x, y);
        if open(tile)? {
            usable.insert(tile);
        }
    }
    let mut condemned = BTreeSet::new();
    let mut proofs = 0;
    for (_, y, x, at) in candidates {
        let tile = TileCoord::new(x, y);
        if !usable.contains(&tile) || condemned.contains(&tile) {
            continue;
        }
        if prove && proofs < MAX_HIERARCHY_PROOFS {
            proofs += 1;
            if !reachable(tile) {
                let mut pending = vec![tile];
                condemned.insert(tile);
                while let Some(from) = pending.pop() {
                    for (dx, dy) in NEIGHBOURS {
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if usable.contains(&to) && !condemned.contains(&to) && linked(from, to)? {
                            condemned.insert(to);
                            pending.push(to);
                        }
                    }
                }
                continue;
            }
        }
        return Ok(Some(at));
    }
    Ok(None)
}

mod prefetch;
#[cfg(test)]
mod tests;
