//! Bounded, canonical search for a firing tile in the attacker's component.
use super::{MELEE_RANGE_SUBUNITS, tick::Goal};
use crate::{GameWorld, GameWorldError};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_map::HierarchicalOutcome;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

/// Bounds both search work and retained frontier entries, independent of map size.
const MAX_ATTACK_SEARCH_TILES: usize = 1_024;
/// Targets within this many tiles are checked for reachability over chunk portals.
const HIERARCHY_PROOF_TILES: i64 = 400;
/// Candidate tiles proven over the portal graph per search, so a target in a
/// sealed pocket costs a bounded number of abstract searches.
const MAX_HIERARCHY_PROOFS: u32 = 4;
/// Combat only proves a candidate; the route scheduler owns full route work.
const ATTACK_HIERARCHY_EXPANSIONS: u32 = 256;
pub(super) const MAX_ATTACK_SEARCHES_PER_TICK: u32 = 16;
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
/// Full searches for a unit that already needed the far fallback, per tick.
pub(crate) const MAX_FAR_SEARCHES_PER_TICK: u32 = 4;
/// A far firing tile is reused while the target keeps its tile and the
/// attacker is farther than this: the local search could not meet it anyway.
pub(crate) const FAR_REUSE_TILES: i64 = 64;

/// The firing tile of a far search, valid for one target tile.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FarAttack {
    pub(crate) centre: TileCoord,
    pub(crate) destination: WorldPosition,
}

pub(crate) struct AttackSearch {
    pub(crate) destination: Option<WorldPosition>,
    /// The local search hit its cap and the far fallback decided.
    pub(crate) far: bool,
}

impl GameWorld {
    pub(super) fn reserve_pursuit_search(&mut self, id: EntityId) -> bool {
        if self.combat.pursuit_floor.is_some_and(|floor| id <= floor)
            || !self.reserve_attack_search()
        {
            return false;
        }
        self.combat.pursuit_cursor = Some(id);
        true
    }

    pub(super) fn reserve_attack_search(&mut self) -> bool {
        if self.combat.search_tick != self.tick.0 {
            self.combat.search_tick = self.tick.0;
            self.combat.searches_used = 0;
        }
        if self.combat.searches_used == MAX_ATTACK_SEARCHES_PER_TICK {
            return false;
        }
        self.combat.searches_used += 1;
        self.combat.stats.attack_searches += 1;
        true
    }
    /// Search outward from the attacker, prioritizing travel distance plus a
    /// lower bound on distance still needed to reach attack range. Reaching a
    /// tile proves connectivity; merely being passable near the target does not.
    pub(crate) fn attack_destination(
        &self,
        id: EntityId,
        goal: &Goal,
    ) -> Result<Option<WorldPosition>, GameWorldError> {
        Ok(self.attack_search(id, goal)?.destination)
    }

    pub(crate) fn attack_search(
        &self,
        id: EntityId,
        goal: &Goal,
    ) -> Result<AttackSearch, GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let stats = self.unit_stats(id).ok_or(GameWorldError::UnknownEntity)?;
        let range = super::super::navy::attack_range(&stats);
        let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
        let origin = unit.position.tile_floor();
        let estimate = |at: WorldPosition| {
            // Diagonal steps cost 1448 rather than sqrt(2)*1024. This small
            // discount keeps the Euclidean lower bound below that grid cost.
            (goal.distance_squared(at) as u64)
                .isqrt()
                .saturating_sub(range as u64)
                * 999
                / 1000
        };
        let mut reached = BTreeMap::from([(origin, 0_u64)]);
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
            if reached.get(&tile) != Some(&cost) {
                continue;
            }
            let at = WorldPosition::from_tile_center(tile)
                .map_err(|_| GameWorldError::InvalidPosition)?;
            let distance = goal.distance_squared(at);
            if distance <= range * range && distance >= min * min {
                return Ok(AttackSearch {
                    destination: Some(at),
                    far: false,
                });
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
                if reached.get(&next).is_some_and(|&old| old <= next_cost)
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
                reached.insert(next, next_cost);
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
                destination: self.far_attack_destination(unit.kind, origin, goal, range, min)?,
                far: true,
            });
        }
        Ok(AttackSearch {
            destination: None,
            far: false,
        })
    }

    /// Firing tile for a target beyond the local search: the nearest open tile
    /// to the goal that is in range, canonical order (distance, y, x). Within
    /// `HIERARCHY_PROOF_TILES` the chunk-portal graph also proves that the
    /// attacker can reach its chunk; farther away the walk itself plans legs
    /// as it goes, like any far move order, and a hopeless chase ends at the
    /// next repath. Work per call is bounded by the candidate window and the
    /// abstract-node cap, never by the distance.
    fn far_attack_destination(
        &self,
        kind: crate::UnitKind,
        origin: TileCoord,
        goal: &Goal,
        range: i64,
        min: i64,
    ) -> Result<Option<WorldPosition>, GameWorldError> {
        let centre = goal.centre().tile_floor();
        let reach = i32::try_from(range / MELEE_RANGE_SUBUNITS).unwrap_or(0) + 2;
        let span = match goal {
            Goal::Unit(_) => 0,
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
            <= HIERARCHY_PROOF_TILES;
        let prove = near && !kind.is_naval();
        pick_firing_tile(
            candidates,
            prove,
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
                !matches!(
                    self.terrain
                        .hierarchical_route(origin, tile, ATTACK_HIERARCHY_EXPANSIONS),
                    Some(HierarchicalOutcome::Unreachable)
                )
            },
        )
    }
}

/// The first candidate (in canonical order) that is open and, when `prove`,
/// reachable. A tile proven unreachable condemns every candidate joined to it
/// by clear steps through the candidate window without another proof, so an
/// islet with dozens of firing tiles costs one proof, not dozens. Only
/// `MAX_HIERARCHY_PROOFS` separate groups are proven per search; past that the
/// next open candidate is taken unproven, as beyond `HIERARCHY_PROOF_TILES`:
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

#[cfg(test)]
mod tests;
