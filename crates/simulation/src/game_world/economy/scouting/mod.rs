//! Server-owned cavalry exploration through known, reachable land frontiers.
use crate::{EconomyError, GameWorld, TileVisibility, UnitKind, UnitTask};
use aoe_core::{EntityId, PlayerId, TileCoord, WorldPosition};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    ops::Bound::{Excluded, Unbounded},
};

#[cfg(test)]
mod tests;

const SEARCH_WORK: usize = 128;
const ACTIVE_SEARCHES: usize = 16;
const JOBS_PER_TICK: usize = 8;
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

#[derive(Debug, Default)]
pub(crate) struct AutoScout {
    search: Option<Search>,
    goal: Option<TileCoord>,
    refused: BTreeSet<TileCoord>,
    sleep_revision: Option<u64>,
}
#[derive(Debug)]
struct Search {
    origin: TileCoord,
    open: BinaryHeap<Reverse<(u64, i32, i32)>>,
    // Distances exist only for the pending frontier. Settled tiles need one
    // bit each, rather than a permanent distance-map entry and a tile cap.
    costs: BTreeMap<TileCoord, u64>,
    settled: BTreeMap<(i32, i32), [u64; 16]>,
}
impl Search {
    fn new(origin: TileCoord) -> Self {
        Self {
            origin,
            open: BinaryHeap::from([Reverse((0, origin.y, origin.x))]),
            costs: BTreeMap::from([(origin, 0)]),
            settled: BTreeMap::new(),
        }
    }

    fn bit(tile: TileCoord) -> ((i32, i32), usize, u64) {
        let side = aoe_map::CHUNK_TILES;
        let at = (tile.y.rem_euclid(side) * side + tile.x.rem_euclid(side)) as usize;
        (
            (tile.x.div_euclid(side), tile.y.div_euclid(side)),
            at / 64,
            1 << (at % 64),
        )
    }

    fn is_settled(&self, tile: TileCoord) -> bool {
        let (key, word, mask) = Self::bit(tile);
        self.settled
            .get(&key)
            .is_some_and(|bits| bits[word] & mask != 0)
    }

    fn settle(&mut self, tile: TileCoord) {
        let (key, word, mask) = Self::bit(tile);
        self.costs.remove(&tile);
        self.settled.entry(key).or_insert([0; 16])[word] |= mask;
    }
}
enum Poll {
    Pending,
    Frontier(TileCoord),
    Exhausted,
}

impl UnitKind {
    pub fn is_cavalry(self) -> bool {
        self == Self::Scout
    }
}

impl GameWorld {
    /// Toggle one living cavalry unit. Manual movement, combat and Stop cancel it.
    pub fn issue_auto_scout(
        &mut self,
        player: PlayerId,
        id: EntityId,
        enabled: bool,
    ) -> Result<(), EconomyError> {
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0)
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !unit.kind.is_cavalry() {
            return Err(EconomyError::WrongUnitKind);
        }
        if enabled == unit.auto_scout {
            return Ok(());
        }
        self.issue_stop(player, id)?;
        if enabled {
            self.economy.scouts.insert(id, AutoScout::default());
            self.units[self.lookup[&id]].state.auto_scout = true;
        }
        Ok(())
    }

    pub(crate) fn cancel_auto_scout(&mut self, id: EntityId) {
        self.economy.scouts.remove(&id);
        self.economy.scout_searches.remove(&id);
        if self.economy.scouts.is_empty() {
            self.economy.scout_cursor = None;
        }
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.auto_scout = false;
        }
    }

    pub(super) fn advance_auto_scout(&mut self, changed: &mut Vec<EntityId>) {
        let ids: Vec<_> = if let Some(cursor) = self.economy.scout_cursor {
            self.economy
                .scouts
                .range((Excluded(cursor), Unbounded))
                .chain(self.economy.scouts.range(..=cursor))
                .take(JOBS_PER_TICK)
                .map(|(&id, _)| id)
                .collect()
        } else {
            self.economy
                .scouts
                .keys()
                .take(JOBS_PER_TICK)
                .copied()
                .collect()
        };
        // A retained cursor provides fair O(log n + 8) admission over active
        // jobs, with at most 1024 frontier heap pops per world tick.
        for id in ids {
            self.economy.scout_cursor = Some(id);
            let Some(unit) = self.unit(id) else {
                self.economy.scouts.remove(&id);
                self.economy.scout_searches.remove(&id);
                continue;
            };
            let index = self.lookup[&id];
            if self.units[index].order.is_some() || unit.planning {
                continue;
            }
            let Some(mut job) = self.economy.scouts.remove(&id) else {
                continue;
            };
            if let Some(goal) = job.goal.take() {
                if self.units[index].last_movement_error.is_some() {
                    if job.refused.len() == 64 {
                        job.refused.pop_first();
                    }
                    job.refused.insert(goal);
                }
                job.search = None;
            }
            let revision = self.player_vision_revision(unit.player);
            if job.sleep_revision == Some(revision) {
                self.economy.scouts.insert(id, job);
                continue;
            }
            job.sleep_revision = None;
            let from = unit.position.tile_floor();
            if job.search.as_ref().is_none_or(|s| s.origin != from) {
                if job.search.is_none() && self.economy.scout_searches.len() == ACTIVE_SEARCHES {
                    self.economy.scouts.insert(id, job);
                    continue;
                }
                self.economy.scout_searches.insert(id);
                job.search = Some(Search::new(from));
            }
            let outcome = match job.search.as_mut() {
                Some(search) if !self.fog_disabled() => {
                    self.poll_frontier(unit.player, unit.kind, search, &job.refused)
                }
                _ => Poll::Exhausted,
            };
            match outcome {
                Poll::Pending => {}
                Poll::Exhausted => {
                    self.economy.scout_searches.remove(&id);
                    job.search = None;
                    job.sleep_revision = Some(revision);
                }
                Poll::Frontier(tile) => {
                    self.economy.scout_searches.remove(&id);
                    job.search = None;
                    let issued = WorldPosition::from_tile_center(tile)
                        .ok()
                        .is_some_and(|p| self.issue_task_move(id, p).is_ok());
                    if issued {
                        job.goal = Some(tile);
                        self.units[index].state.task = UnitTask::Moving;
                        changed.push(id);
                    } else {
                        if job.refused.len() == 64 {
                            job.refused.pop_first();
                        }
                        job.refused.insert(tile);
                    }
                }
            }
            self.economy.scouts.insert(id, job);
        }
    }

    fn poll_frontier(
        &self,
        player: PlayerId,
        kind: UnitKind,
        search: &mut Search,
        refused: &BTreeSet<TileCoord>,
    ) -> Poll {
        for _ in 0..SEARCH_WORK {
            let Some(Reverse((cost, y, x))) = search.open.pop() else {
                return Poll::Exhausted;
            };
            let tile = TileCoord::new(x, y);
            if search.costs.get(&tile) != Some(&cost) {
                continue;
            }
            search.settle(tile);
            let valid = |t: TileCoord| {
                t.x >= 0
                    && t.y >= 0
                    && t.x < self.config.width_tiles
                    && t.y < self.config.height_tiles
            };
            if tile != search.origin
                && !refused.contains(&tile)
                && NEIGHBOURS[..4].iter().any(|(dx, dy)| {
                    let next = TileCoord::new(x + dx, y + dy);
                    valid(next) && self.visibility(player, next) == TileVisibility::Unexplored
                })
            {
                return Poll::Frontier(tile);
            }
            for (dx, dy) in NEIGHBOURS {
                let next = TileCoord::new(x + dx, y + dy);
                // Never query hidden ground to choose an exploration route.
                if dx != 0
                    && dy != 0
                    && [TileCoord::new(x + dx, y), TileCoord::new(x, y + dy)]
                        .iter()
                        .any(|&t| self.visibility(player, t) == TileVisibility::Unexplored)
                {
                    continue;
                }
                if !valid(next)
                    || search.is_settled(next)
                    || self.visibility(player, next) == TileVisibility::Unexplored
                    || !self.building_step_clear(tile, next)
                    || !self.unit_step_clear(kind, tile, next).unwrap_or(false)
                {
                    continue;
                }
                let next_cost = cost + if dx == 0 || dy == 0 { 1024 } else { 1448 };
                let previous = search.costs.get(&next);
                if previous.is_some_and(|c| *c <= next_cost) {
                    continue;
                }
                search.costs.insert(next, next_cost);
                search.open.push(Reverse((next_cost, next.y, next.x)));
            }
        }
        Poll::Pending
    }
}

impl AutoScout {
    pub(super) fn update_hash(&self, hash: &mut blake3::Hasher) {
        let tile = |hash: &mut blake3::Hasher, t: TileCoord| {
            hash.update(&t.x.to_le_bytes());
            hash.update(&t.y.to_le_bytes());
        };
        hash.update(&self.sleep_revision.unwrap_or(u64::MAX).to_le_bytes());
        hash.update(&[u8::from(self.goal.is_some())]);
        if let Some(goal) = self.goal {
            tile(hash, goal);
        }
        hash.update(&(self.refused.len() as u64).to_le_bytes());
        for &t in &self.refused {
            tile(hash, t);
        }
        hash.update(&[u8::from(self.search.is_some())]);
        if let Some(search) = &self.search {
            tile(hash, search.origin);
            hash.update(&(search.costs.len() as u64).to_le_bytes());
            for (&t, cost) in &search.costs {
                tile(hash, t);
                hash.update(&cost.to_le_bytes());
            }
            hash.update(&(search.settled.len() as u64).to_le_bytes());
            for (&(x, y), bits) in &search.settled {
                tile(hash, TileCoord::new(x, y));
                for word in bits {
                    hash.update(&word.to_le_bytes());
                }
            }
            let open = search.open.clone().into_sorted_vec();
            hash.update(&(open.len() as u64).to_le_bytes());
            for Reverse((cost, y, x)) in open {
                hash.update(&cost.to_le_bytes());
                tile(hash, TileCoord::new(x, y));
            }
        }
    }
}
