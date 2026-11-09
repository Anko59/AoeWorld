//! The same pins on hand-made ground, where each edge of the search can be
//! placed exactly: records requeued at a lower cost, a goal sealed in a
//! pocket, an exhausted flood, ground that changes between two polls,
//! planners polled in turn on one thread, and a ground read that fails.
use super::*;
use crate::EnvironmentPageError;
use aoe_core::TileRect;
use std::cell::RefCell;

type Answer<T> = Result<T, EnvironmentPageError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Read {
    Walkable(TileCoord),
    Edge(TileCoord, TileCoord),
    /// The cost of a step into this tile.
    Cost(TileCoord),
}

/// A 64 x 64 yard without a map generator, as uniform worlds and ships have.
struct Yard {
    /// Tiles that are never walkable.
    solid: fn(TileCoord) -> bool,
    /// Tiles that cost three times as much to enter.
    rough: fn(TileCoord) -> bool,
    /// A read that fails, as a missing page does.
    missing: Option<Read>,
    /// Every walkability answer given for `watched`, in order. The planner's
    /// memo asks at most once per poll, so this counts the polls that asked.
    watched: TileCoord,
    seen: RefCell<Vec<bool>>,
}

impl Yard {
    fn open() -> Self {
        Self {
            solid: |_| false,
            rough: |_| false,
            missing: None,
            watched: TileCoord::new(-1, -1),
            seen: RefCell::default(),
        }
    }
}

impl RouteTerrain for Yard {
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        if self.missing == Some(Read::Walkable(tile)) {
            return Err(EnvironmentPageError::Missing);
        }
        let walkable = (0..64).contains(&tile.x)
            && (0..64).contains(&tile.y)
            && !(self.solid)(tile)
            && !overlay.blocks_tile(tile);
        if tile == self.watched {
            self.seen.borrow_mut().push(walkable);
        }
        Ok(walkable)
    }

    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        _: &dyn Fn() -> bool,
    ) -> Answer<bool> {
        if self.missing == Some(Read::Edge(from, to)) {
            return Err(EnvironmentPageError::Missing);
        }
        Ok(true)
    }

    fn route_step_cost(&self, from: TileCoord, to: TileCoord, _: &dyn Fn() -> bool) -> Answer<u32> {
        if self.missing == Some(Read::Cost(to)) {
            return Err(EnvironmentPageError::Missing);
        }
        let base = if from.x == to.x || from.y == to.y {
            ORTHOGONAL_COST
        } else {
            DIAGONAL_COST
        };
        Ok(if (self.rough)(to) { base * 3 } else { base })
    }
}

fn tile(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn one(at: TileCoord) -> TileRect {
    TileRect::from_xywh(at.x, at.y, 1, 1)
}

fn rough(at: TileCoord) -> bool {
    (at.x * 3 + at.y * 5) % 7 < 2
}

/// A wall along `x == 12` from the northern edge down to `y == 40`.
fn walled() -> ResourceOverlay {
    let mut overlay = ResourceOverlay::default();
    overlay
        .add_footprint(TileRect::from_xywh(12, 0, 1, 41))
        .unwrap();
    overlay
}

#[test]
fn uneven_ground_requeues_records_at_a_lower_cost() {
    let scenario = |budget, lookahead| {
        let yard = Yard {
            rough,
            ..Yard::open()
        };
        let mut planner = RoutePlanner::new(tile(6, 9), tile(17, 15), 65_536);
        let overlay = ResourceOverlay::empty();
        let run = drive(&mut planner, &yard, overlay, budget, lookahead);
        // One leg below the pocket check: two validations, a pop and eight
        // probes for each expansion, the goal's pop, and two reconstruction
        // units for each step. Whatever is left popped a superseded node.
        let steps = run.tiles.len() as u32 - 1;
        assert!(planner.work() - 2 * steps < 1_024, "the pocket check ran");
        let superseded = planner.work() - 3 - 9 * planner.expansions() - 2 * steps;
        assert_eq!(superseded, 10, "superseded frontier nodes popped");
        run
    };
    let pinned = [
        "budget=8192 calls=2 end=complete work=584 expansions=61 peak=285 tiles=12 last=17,15 near=- cost=15856 route=6f3db9325b0a6cac trace=da8614a1c2ef0e62",
        "budget=127 calls=6 end=complete work=584 expansions=61 peak=285 tiles=12 last=17,15 near=- cost=15856 route=6f3db9325b0a6cac trace=61f074660053e5c8",
        "budget=1 calls=585 end=complete work=584 expansions=61 peak=285 tiles=12 last=17,15 near=- cost=15856 route=6f3db9325b0a6cac trace=523c7b46cf77a119",
    ];
    let runs = [(FAST_POLL, false), (127, false), (1, false)];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn a_far_order_in_a_yard_is_planned_leg_by_leg_and_ahead() {
    let (origin, goal) = (tile(2, 2), tile(60, 60));
    let scenario = |budget, lookahead| {
        let mut planner = RoutePlanner::new(origin, goal, 1_000_000);
        let run = drive(
            &mut planner,
            &Yard::open(),
            ResourceOverlay::empty(),
            budget,
            lookahead,
        );
        assert_eq!(run.cost, heuristic(origin, goal), "both legs are exact");
        run
    };
    let pinned = [
        "budget=127 calls=8 end=complete work=642 expansions=58 peak=416 tiles=59 last=60,60 near=- cost=83984 route=e650de99ad5d1014 trace=0781c4bbebf54696",
        "budget=127+ahead calls=8 end=complete work=642 expansions=58 peak=416 tiles=59 last=60,60 near=- cost=83984 route=e650de99ad5d1014 trace=48664f507fde134b",
        "budget=1+ahead calls=644 end=complete work=642 expansions=58 peak=416 tiles=59 last=60,60 near=- cost=83984 route=e650de99ad5d1014 trace=157789d9de49221b",
    ];
    let runs = [(127, false), (127, true), (1, true)];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn a_goal_sealed_in_a_small_pocket_ends_by_its_reverse_proof() {
    let scenario = |budget, lookahead| {
        let yard = Yard {
            solid: |at| (at.x - 40).abs().max((at.y - 40).abs()) == 6,
            ..Yard::open()
        };
        let mut planner = RoutePlanner::new(tile(40, 20), tile(40, 40), 1_000_000);
        let overlay = ResourceOverlay::empty();
        drive(&mut planner, &yard, overlay, budget, lookahead)
    };
    let pinned = [
        "budget=4096 calls=1 end=unreachable work=1992 expansions=108 peak=466 tiles=1 last=40,20 near=- cost=0 route=5277af86cf0638ed trace=bc80bf37a95f3ed1",
        "budget=127 calls=16 end=unreachable work=1992 expansions=108 peak=466 tiles=1 last=40,20 near=- cost=0 route=5277af86cf0638ed trace=6cfad60a0d47b2a0",
        "budget=1 calls=1993 end=unreachable work=1992 expansions=108 peak=466 tiles=1 last=40,20 near=- cost=0 route=5277af86cf0638ed trace=5c1f46be4a07c973",
    ];
    let runs = [(4_096, false), (127, false), (1, false)];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn an_exhausted_flood_reports_the_nearest_tile_it_closed() {
    let scenario = |budget, lookahead| {
        let yard = Yard {
            solid: |at| (at.x - 12).abs().max((at.y - 12).abs()) == 3,
            ..Yard::open()
        };
        let mut planner = RoutePlanner::new(tile(10, 12), tile(40, 12), 1_000_000);
        let overlay = ResourceOverlay::empty();
        let run = drive(&mut planner, &yard, overlay, budget, lookahead);
        assert_eq!(planner.nearest_reachable(), Some(tile(14, 12)));
        run
    };
    let pinned = [
        "budget=4096 calls=1 end=unreachable work=241 expansions=25 peak=50 tiles=1 last=10,12 near=14,12 cost=0 route=0e380c5a938e3f4c trace=1eb28ee19a1fe467",
        "budget=1 calls=242 end=unreachable work=241 expansions=25 peak=50 tiles=1 last=10,12 near=14,12 cost=0 route=0e380c5a938e3f4c trace=cf7449ccadd5dd46",
    ];
    assert_pins(&lines(&[(4_096, false), (1, false)], scenario), &pinned);
}

/// Polls one work unit at a time until the search has asked the yard about
/// its watched tile.
fn poll_until_watched(planner: &mut RoutePlanner, yard: &Yard, overlay: &ResourceOverlay) {
    while yard.seen.borrow().is_empty() {
        assert_eq!(
            planner.poll(yard, overlay, 1, &|| false),
            RoutePlannerPoll::Pending
        );
    }
}

#[test]
fn ground_opened_between_two_polls_is_read_again() {
    // The simulation changes its one overlay in place between ticks, so a
    // memo kept from an earlier poll would answer for ground that is gone.
    let gap = tile(12, 10);
    let mut overlay = walled();
    let yard = Yard {
        watched: gap,
        ..Yard::open()
    };
    let mut planner = RoutePlanner::new(tile(8, 10), tile(16, 10), 65_536);
    poll_until_watched(&mut planner, &yard, &overlay);
    assert_eq!(*yard.seen.borrow(), [false], "the wall tile was read once");
    assert!(!planner.touches(one(gap)), "a blocked tile is not retained");
    overlay.remove_footprint(one(gap));
    let run = drive(&mut planner, &yard, &overlay, 1, false);
    let seen = yard.seen.borrow();
    assert!(seen.len() > 1 && seen[1..].iter().all(|open| *open));
    let straight: Vec<_> = (8..=16).map(|x| tile(x, 10)).collect();
    assert_eq!(run.tiles, straight, "the opened gap is on the route");
    let pinned = [
        "calls=59 end=complete work=91 expansions=8 peak=72 tiles=9 last=16,10 near=- cost=8192 route=1e9df5d78a7cc148 trace=3936e9d08f96b6b8",
    ];
    assert_pins(&[run.line], &pinned);
}

#[test]
fn a_goal_built_over_between_two_polls_is_read_again() {
    let goal = tile(12, 10);
    let mut overlay = ResourceOverlay::default();
    let yard = Yard {
        watched: goal,
        ..Yard::open()
    };
    let mut planner = RoutePlanner::new(tile(20, 10), goal, 65_536);
    poll_until_watched(&mut planner, &yard, &overlay);
    assert_eq!(*yard.seen.borrow(), [true], "validation read an open goal");
    assert_eq!(planner.work(), 2);
    overlay.add_footprint(one(goal)).unwrap();
    let run = drive(&mut planner, &yard, &overlay, 127, false);
    let seen = yard.seen.borrow();
    assert!(seen.len() > 1 && seen[1..].iter().all(|open| !*open));
    assert!(!run.tiles.contains(&goal));
    let pinned = [
        "calls=9 end=unreachable work=1032 expansions=107 peak=231 tiles=1 last=20,10 near=- cost=0 route=d4ee7d5084109fda trace=de9c13b914c29eb1",
    ];
    assert_pins(&[run.line], &pinned);
}

#[test]
fn planners_polled_in_turn_keep_their_own_ground() {
    // Two orders between the same two tiles: one around a wall on even
    // ground, one over uneven ground without the wall. Polled in turn on one
    // thread, each must end exactly as it does alone.
    let (origin, goal) = (tile(8, 10), tile(16, 10));
    let (wall, bare) = (walled(), ResourceOverlay::default());
    let even = Yard::open();
    let uneven = Yard {
        rough,
        ..Yard::open()
    };
    let alone = |yard: &Yard, overlay: &ResourceOverlay| {
        drive(
            &mut RoutePlanner::new(origin, goal, 1_000_000),
            yard,
            overlay,
            127,
            false,
        )
    };
    let (around, across) = (alone(&even, &wall), alone(&uneven, &bare));
    assert!(
        around.tiles.contains(&tile(12, 41)),
        "around the wall's end"
    );
    assert!(across.tiles.len() < around.tiles.len());

    let mut first = RoutePlanner::new(origin, goal, 1_000_000);
    let mut second = RoutePlanner::new(origin, goal, 1_000_000);
    let mut walled_order = Driver::new(&mut first, &even, &wall, 127, false);
    let mut bare_order = Driver::new(&mut second, &uneven, &bare, 127, false);
    while walled_order.end.is_none() || bare_order.end.is_none() {
        walled_order.turn();
        bare_order.turn();
    }
    let (walled_order, bare_order) = (walled_order.finish(), bare_order.finish());
    assert_eq!(walled_order.line, around.line);
    assert_eq!(bare_order.line, across.line);
    assert_eq!(walled_order.tiles, around.tiles);
    assert_eq!(bare_order.tiles, across.tiles);
    let pinned = [
        "calls=102 end=complete work=12622 expansions=522 peak=1285 tiles=65 last=16,10 near=- cost=68080 route=dd028c46eacfbe91 trace=183fcbc9153b252e",
        "calls=5 end=complete work=415 expansions=43 peak=191 tiles=9 last=16,10 near=- cost=11936 route=8e6caac1d64a7661 trace=6f543955b8643b99",
    ];
    assert_pins(&[around.line, across.line], &pinned);
}

#[test]
fn a_failed_ground_read_ends_the_search_at_the_probe_that_asked() {
    // The planner's work is hashed, so the unit at which a missing page
    // surfaces is part of the result: ground must not be read before the
    // probe that is charged for it.
    let scenario = |missing: Read| {
        move |budget, lookahead| {
            let yard = Yard {
                missing: Some(missing),
                ..Yard::open()
            };
            let mut planner = RoutePlanner::new(tile(8, 10), tile(16, 10), 65_536);
            let overlay = ResourceOverlay::empty();
            drive(&mut planner, &yard, overlay, budget, lookahead)
        }
    };
    let runs = [(127, false), (1, false)];
    let ahead = tile(12, 10);
    let observed = [
        lines(&runs, scenario(Read::Walkable(ahead))),
        lines(&runs, scenario(Read::Edge(tile(11, 10), ahead))),
        lines(&runs, scenario(Read::Cost(ahead))),
        // Two rows off the straight line: ground the search never reads.
        lines(&runs, scenario(Read::Walkable(tile(9, 12)))),
    ]
    .concat();
    let pinned = [
        "budget=127 calls=1 end=environment:Missing work=33 expansions=3 peak=28 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=90e7a4a9cc03c474",
        "budget=1 calls=33 end=environment:Missing work=33 expansions=3 peak=28 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=5d8ab3bb4db9101a",
        "budget=127 calls=1 end=environment:Missing work=33 expansions=3 peak=28 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=90e7a4a9cc03c474",
        "budget=1 calls=33 end=environment:Missing work=33 expansions=3 peak=28 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=5d8ab3bb4db9101a",
        "budget=127 calls=1 end=environment:Missing work=35 expansions=3 peak=29 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=6567eba2c4ac5d44",
        "budget=1 calls=35 end=environment:Missing work=35 expansions=3 peak=29 tiles=1 last=8,10 near=- cost=0 route=36cbcfc6a58bce89 trace=c3f99cbc43494ae4",
        "budget=127 calls=2 end=complete work=91 expansions=8 peak=76 tiles=9 last=16,10 near=- cost=8192 route=1e9df5d78a7cc148 trace=0175b441b94a1a67",
        "budget=1 calls=92 end=complete work=91 expansions=8 peak=76 tiles=9 last=16,10 near=- cost=8192 route=1e9df5d78a7cc148 trace=59b019d6677bb3c7",
    ];
    assert_pins(&observed, &pinned);
}
