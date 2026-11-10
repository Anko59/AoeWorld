//! Resumable A*: one heap pop, traceback edge or laid site per operation.
use super::plan::Site;
use super::road_cache::Lookup;
use super::roads::View;
use super::*;
use std::{cmp::Reverse, collections::BinaryHeap};
const EXPANSIONS: usize = 40_000;
const NEW_BRIDGE_COST: u32 = 60;
const BRIDGE_TILE_COST: u32 = 3;
const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];
const TURN: u32 = 3;
type State = (TileCoord, u8);
type Priority = Reverse<(u32, u32, i32, i32, u8)>;
pub(super) struct Search {
    pub(super) node: TileRect,
    pub(super) edge: Vec<TileCoord>,
    tier: u8,
    aim: Option<TileCoord>,
    heap: BinaryHeap<Priority>,
    best: Lookup<State, (u32, Option<(State, bool)>)>,
    settled: usize,
    phase: u8,
    at: Option<State>,
    tiles: Vec<TileCoord>,
    spans: Vec<(TileCoord, TileCoord)>,
    banks: BTreeSet<TileCoord>,
    cursor: usize,
    history: blake3::Hasher,
}
impl Search {
    pub(super) fn new(
        view: &View<'_>,
        starts: &[TileCoord],
        node: TileRect,
        edge: Vec<TileCoord>,
        tier: u8,
    ) -> Self {
        let aim = starts.first().and_then(|s| {
            view.goals
                .iter()
                .min_by_key(|g| (super::super::placement::distance_squared(**g, *s), g.y, g.x))
                .copied()
        });
        let mut search = Self {
            node,
            edge,
            tier,
            aim,
            heap: BinaryHeap::new(),
            best: Lookup::default(),
            settled: 0,
            phase: 0,
            at: None,
            tiles: Vec::new(),
            spans: Vec::new(),
            banks: BTreeSet::new(),
            cursor: 0,
            history: blake3::Hasher::new(),
        };
        for &t in starts.iter().filter(|t| view.walkable(**t)) {
            search.record((t, 4), 0, None);
            search.best.insert((t, 4), (0, None));
            search.heap.push(Reverse((search.h(t), 0, t.y, t.x, 4)));
        }
        search
    }
    fn h(&self, t: TileCoord) -> u32 {
        self.aim
            .map_or(0, |a| ((t.x - a.x).abs() + (t.y - a.y).abs()) as u32)
    }
    fn record(&mut self, state: State, cost: u32, parent: Option<(State, bool)>) {
        self.history.update(&[1, state.1]);
        for n in [state.0.x, state.0.y] {
            self.history.update(&n.to_le_bytes());
        }
        self.history.update(&cost.to_le_bytes());
        if let Some((p, b)) = parent {
            self.history.update(&[1, p.1, u8::from(b)]);
            for n in [p.0.x, p.0.y] {
                self.history.update(&n.to_le_bytes());
            }
        } else {
            self.history.update(&[0]);
        }
    }
    pub(super) fn hash(&self, h: &mut blake3::Hasher) {
        h.update(self.history.finalize().as_bytes());
        h.update(
            format!(
                "{:?}",
                (
                    self.node,
                    &self.edge,
                    self.tier,
                    self.aim,
                    self.settled,
                    self.phase,
                    self.at,
                    &self.tiles,
                    &self.spans,
                    &self.banks,
                    self.cursor
                )
            )
            .as_bytes(),
        );
    }
    pub(super) fn step(&mut self, view: &mut View<'_>) -> Option<bool> {
        match self.phase {
            0 => self.expand(view),
            1 => {
                let at = self.at?;
                if let Some((_, Some((prev, bridge)))) = self.best.get(&at).copied() {
                    if bridge {
                        self.spans.push((prev.0, at.0));
                        self.banks.extend([prev.0, at.0]);
                    }
                    self.tiles.push(prev.0);
                    self.at = Some(prev);
                } else {
                    self.phase = 2;
                    self.cursor = 0;
                }
                None
            }
            2 => {
                if let Some(&tile) = self.tiles.get(self.cursor) {
                    self.cursor += 1;
                    if !self.banks.contains(&tile) {
                        view.lay(tile, self.tier);
                    }
                } else {
                    self.phase = 3;
                    self.cursor = 0;
                }
                None
            }
            _ => {
                if let Some(&(a, b)) = self.spans.get(self.cursor) {
                    self.cursor += 1;
                    let (from, to) = if (a.x, a.y) <= (b.x, b.y) {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    view.out.push(Site {
                        end: Some(to),
                        ..Site::new(BuildingKind::Bridge, from, self.tier)
                    });
                    view.goals.extend([a, b]);
                    view.planned_spans.extend([a, b]);
                    let len = (a.x - b.x).abs().max((a.y - b.y).abs()) as u32 + 1;
                    view.fixed.insert(a, (b, len));
                    view.fixed.insert(b, (a, len));
                    None
                } else {
                    Some(true)
                }
            }
        }
    }
    fn expand(&mut self, view: &mut View<'_>) -> Option<bool> {
        let Some(Reverse((_, cost, y, x, heading))) = self.heap.pop() else {
            return Some(false);
        };
        self.history.update(&[2, heading]);
        for n in [x, y] {
            self.history.update(&n.to_le_bytes());
        }
        self.history.update(&cost.to_le_bytes());
        let tile = TileCoord::new(x, y);
        let here = (tile, heading);
        if self.best.get(&here).is_none_or(|(c, _)| *c < cost) {
            return None;
        }
        if view.goals.contains(&tile) {
            self.at = Some(here);
            self.tiles.push(tile);
            self.phase = 1;
            return None;
        }
        self.settled += 1;
        if self.settled > EXPANSIONS {
            return Some(false);
        }
        if let Some(&(other, len)) = view.fixed.get(&tile) {
            let d = match ((other.x - tile.x).signum(), (other.y - tile.y).signum()) {
                (-1, _) => 1,
                (1, _) => 2,
                (_, 1) => 3,
                _ => 0,
            };
            // Crossing a bridge already laid costs only its walk.
            let total = cost + len.max(1);
            if self.best.get(&(other, d)).is_none_or(|(c, _)| total < *c) {
                self.record((other, d), total, Some((here, true)));
                self.best.insert((other, d), (total, Some((here, true))));
                self.heap
                    .push(Reverse((total + self.h(other), total, other.y, other.x, d)));
            }
        }
        for (d, (dx, dy)) in DIRS.into_iter().enumerate() {
            let d = d as u8;
            let turn = if heading != 4 && heading != d {
                TURN
            } else {
                0
            };
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            let (to, step, bridge) = if view.walkable(next) && view.crossable(tile, next) {
                (next, 1, false)
            } else if !view.avoid.contains(&next)
                && view.w.building_at(next).is_none()
                && !view.passable(next)
            {
                // Water (or other barrier) ahead: look for the far bank.
                let far = super::roads::bank_steps().find_map(|k| {
                    let m = TileCoord::new(tile.x + dx * k, tile.y + dy * k);
                    if view.free(m) {
                        Some((k, m))
                    } else if view.passable(m) || view.w.building_at(m).is_some() {
                        Some((0, m))
                    } else {
                        None
                    }
                });
                match far {
                    Some((k, m)) if k > 0 && view.bridgeable(tile, m) => {
                        (m, NEW_BRIDGE_COST + BRIDGE_TILE_COST * (k as u32 - 1), true)
                    }
                    _ => continue,
                }
            } else {
                continue;
            };
            let total = cost + step + turn;
            let state = (to, d);
            if self.best.get(&state).is_none_or(|(c, _)| total < *c) {
                self.record(state, total, Some((here, bridge)));
                self.best.insert(state, (total, Some((here, bridge))));
                self.heap
                    .push(Reverse((total + self.h(to), total, to.y, to.x, d)));
            }
        }

        None
    }
}
