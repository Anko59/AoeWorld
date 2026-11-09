//! Regression pins of the planner's leg search: the exact route, the work
//! charged and the planner hash after every call. They are the current
//! output. A change to the search that is meant to be result-identical leaves
//! every line alone; a change that is meant to move one regenerates it from
//! the failure message, which prints every observed line.
use super::*;
use crate::RouteTerrain;

#[path = "leg_search_fixtures.rs"]
mod fixtures;
#[path = "leg_search_synthetic.rs"]
mod synthetic;

/// The simulation's fast poll (`FAST_ROUTE_WORK`).
const FAST_POLL: u32 = 8_192;

fn digest(hash: &blake3::Hasher) -> String {
    hash.finalize().to_hex()[..16].to_string()
}

fn at(tile: Option<TileCoord>) -> String {
    tile.map_or("-".to_string(), |tile| format!("{},{}", tile.x, tile.y))
}

fn hash_tile(hash: &mut blake3::Hasher, tile: Option<TileCoord>) {
    hash.update(&[u8::from(tile.is_some())]);
    if let Some(tile) = tile {
        hash.update(&tile.x.to_le_bytes());
        hash.update(&tile.y.to_le_bytes());
    }
}

/// Folds one call into the trace: what it returned (`None` for a lookahead,
/// which returns nothing) and everything the planner shows afterwards.
fn note(trace: &mut blake3::Hasher, planner: &RoutePlanner, result: Option<&RoutePlannerPoll>) {
    match result {
        None => trace.update(&[255]),
        Some(RoutePlannerPoll::Pending) => trace.update(&[0]),
        Some(RoutePlannerPoll::Path(path)) => {
            trace.update(&[1]);
            trace.update(&(path.tiles.len() as u64).to_le_bytes());
            for tile in &path.tiles {
                hash_tile(trace, Some(*tile));
            }
            trace.update(&path.cost.to_le_bytes())
        }
        Some(RoutePlannerPoll::Complete) => trace.update(&[2]),
        Some(RoutePlannerPoll::InvalidDestination) => trace.update(&[3]),
        Some(RoutePlannerPoll::Unreachable) => trace.update(&[4]),
        Some(RoutePlannerPoll::SearchLimit) => trace.update(&[5]),
        Some(RoutePlannerPoll::Environment(error)) => {
            trace.update(&[6]);
            trace.update(format!("{error:?}").as_bytes())
        }
    };
    trace.update(&planner.work().to_le_bytes());
    trace.update(&planner.leg_work().to_le_bytes());
    trace.update(&planner.expansions().to_le_bytes());
    trace.update(&(planner.peak_retained_entries() as u64).to_le_bytes());
    trace.update(&[
        u8::from(planner.is_terminal()),
        u8::from(planner.requires_search_slot()),
        u8::from(planner.wants_lookahead()),
        u8::from(planner.has_route_continuation()),
        u8::from(planner.has_endpoint_refinement()),
        u8::from(planner.preparing_guidance()),
    ]);
    hash_tile(trace, planner.certified_destination());
    hash_tile(trace, planner.nearest_reachable());
    let mut state = blake3::Hasher::new();
    planner.update_hash(&mut state);
    trace.update(state.finalize().as_bytes());
}

/// A planner driven to its end.
struct Run {
    /// Calls made, how it ended, work charged, tiles expanded, peak retained
    /// entries, the route emitted, and the digest of every call's `note`.
    line: String,
    tiles: Vec<TileCoord>,
    cost: u64,
}

/// One planner polled a turn at a time, so that several can take turns.
struct Driver<'a> {
    planner: &'a mut RoutePlanner,
    terrain: &'a dyn RouteTerrain,
    overlay: &'a ResourceOverlay,
    budget: u32,
    lookahead: bool,
    trace: blake3::Hasher,
    tiles: Vec<TileCoord>,
    cost: u64,
    calls: u32,
    end: Option<String>,
}

impl<'a> Driver<'a> {
    fn new(
        planner: &'a mut RoutePlanner,
        terrain: &'a dyn RouteTerrain,
        overlay: &'a ResourceOverlay,
        budget: u32,
        lookahead: bool,
    ) -> Self {
        let tiles = vec![planner.origin()];
        Self {
            planner,
            terrain,
            overlay,
            budget,
            lookahead,
            trace: blake3::Hasher::new(),
            tiles,
            cost: 0,
            calls: 0,
            end: None,
        }
    }

    /// One `poll` with the budget and, with `lookahead`, one `plan_ahead`
    /// after an emitted segment, as the movement step does. Nothing once the
    /// planner has ended.
    fn turn(&mut self) {
        if self.end.is_some() {
            return;
        }
        assert!(self.calls < 4_000_000, "the planner never ended");
        let before = self.planner.work();
        let result = self
            .planner
            .poll(self.terrain, self.overlay, self.budget, &|| false);
        assert!(
            self.planner.work() - before <= self.budget,
            "a poll overspent"
        );
        self.calls += 1;
        note(&mut self.trace, self.planner, Some(&result));
        self.end = match result {
            RoutePlannerPoll::Pending => None,
            RoutePlannerPoll::Path(path) => {
                assert_eq!(path.tiles.first(), self.tiles.last(), "segments must join");
                self.tiles.extend(path.tiles.into_iter().skip(1));
                self.cost += path.cost;
                if self.lookahead && self.planner.wants_lookahead() {
                    self.planner
                        .plan_ahead(self.terrain, self.overlay, self.budget, &|| false);
                    self.calls += 1;
                    note(&mut self.trace, self.planner, None);
                }
                None
            }
            RoutePlannerPoll::Complete => Some("complete".to_string()),
            RoutePlannerPoll::InvalidDestination => Some("invalid".to_string()),
            RoutePlannerPoll::Unreachable => Some("unreachable".to_string()),
            RoutePlannerPoll::SearchLimit => Some("limit".to_string()),
            RoutePlannerPoll::Environment(error) => Some(format!("environment:{error:?}")),
        };
    }

    fn finish(mut self) -> Run {
        while self.end.is_none() {
            self.turn();
        }
        assert!(self.planner.is_terminal());
        let mut route = blake3::Hasher::new();
        for tile in &self.tiles {
            hash_tile(&mut route, Some(*tile));
        }
        route.update(&self.cost.to_le_bytes());
        let line = format!(
            "calls={} end={} work={} expansions={} peak={} tiles={} last={} near={} cost={} \
             route={} trace={}",
            self.calls,
            self.end.as_deref().unwrap_or("-"),
            self.planner.work(),
            self.planner.expansions(),
            self.planner.peak_retained_entries(),
            self.tiles.len(),
            at(self.tiles.last().copied()),
            at(self.planner.nearest_reachable()),
            self.cost,
            digest(&route),
            digest(&self.trace),
        );
        Run {
            line,
            tiles: self.tiles,
            cost: self.cost,
        }
    }
}

/// Polls `planner` with `budget` until it ends.
fn drive(
    planner: &mut RoutePlanner,
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    budget: u32,
    lookahead: bool,
) -> Run {
    Driver::new(planner, terrain, overlay, budget, lookahead).finish()
}

/// Compares every line before failing, so one run shows every pin that moved.
fn assert_pins(observed: &[String], pinned: &[&str]) {
    if observed
        .iter()
        .map(String::as_str)
        .ne(pinned.iter().copied())
    {
        panic!(
            "leg search pins moved.\nobserved:\n{}\npinned:\n{}",
            observed.join("\n"),
            pinned.join("\n")
        );
    }
}

/// Runs one scenario under each `(budget, lookahead)`, every run on a fresh
/// fixture so that no run sees what another prepared or cached.
fn lines(runs: &[(u32, bool)], scenario: impl Fn(u32, bool) -> Run) -> Vec<String> {
    runs.iter()
        .map(|&(budget, lookahead)| {
            let ahead = if lookahead { "+ahead" } else { "" };
            format!(
                "budget={budget}{ahead} {}",
                scenario(budget, lookahead).line
            )
        })
        .collect()
}
