//! Heap traffic of the route planner's per-poll terrain memo
//! (`navigation/planner/memo.rs`).
//!
//! A leg search memoizes the ground it reads in three hash maps. They were
//! built empty for every poll and regrown by rehash, and one simulation tick
//! makes eight or more polls, most of them the first poll of a fresh planner.
//! A poll must reuse the memo an earlier poll left on its thread, capacity
//! included, whichever planner made that poll.
//!
//! Only the test thread's requests are counted, so the counts are exact under
//! `cargo test` as well as under nextest.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Allocations and reallocations requested by this thread.
    static REQUESTS: Cell<u64> = const { Cell::new(0) };
}

struct Counting;

fn count() {
    // The cell has no destructor; a thread being torn down is not counted.
    let _ = REQUESTS.try_with(|requests| requests.set(requests.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Heap requests `run` makes on this thread, and its result.
fn heap_requests<T>(run: impl FnOnce() -> T) -> (u64, T) {
    let before = REQUESTS.with(Cell::get);
    let result = run();
    (REQUESTS.with(Cell::get) - before, result)
}

/// Named so that the planner's usual filter, `test(navigation::)`, runs them.
mod navigation {
    mod leg_search_memo {
        use crate::heap_requests;
        use aoe_core::TileCoord;
        use aoe_map::{
            EnvironmentPageError, ResourceOverlay, RoutePlanner, RoutePlannerPoll, RouteTerrain,
        };

        type Answer<T> = Result<T, EnvironmentPageError>;

        fn step_cost(from: TileCoord, to: TileCoord) -> u32 {
            if from.x == to.x || from.y == to.y {
                1_024
            } else {
                1_448
            }
        }

        /// A corridor one tile wide along `y == 0`, and a walkable goal that
        /// nothing leads to. Every corridor tile is farther from the goal than
        /// the origin, so the search walks the corridor one tile per expansion
        /// and never ends its leg.
        struct Corridor;
        const CORRIDOR_ORIGIN: TileCoord = TileCoord::new(0, 0);
        const CORRIDOR_GOAL: TileCoord = TileCoord::new(0, 4);

        impl RouteTerrain for Corridor {
            fn route_walkable(
                &self,
                _: &ResourceOverlay,
                tile: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<bool> {
                Ok((tile.y == 0 && (0..64).contains(&tile.x)) || tile == CORRIDOR_GOAL)
            }
            fn route_edge_clear(
                &self,
                _: TileCoord,
                _: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<bool> {
                Ok(true)
            }
            fn route_step_cost(
                &self,
                from: TileCoord,
                to: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<u32> {
                Ok(step_cost(from, to))
            }
        }

        fn pending(planner: &mut RoutePlanner, terrain: &dyn RouteTerrain, budget: u32) {
            let before = planner.work();
            assert_eq!(
                planner.poll(terrain, ResourceOverlay::empty(), budget, &|| false),
                RoutePlannerPoll::Pending
            );
            assert_eq!(planner.work() - before, budget, "the poll spent its budget");
        }

        /// Validates both ends and expands the origin: the frontier and record
        /// maps of the search exist afterwards.
        fn corridor_planner() -> RoutePlanner {
            let mut planner = RoutePlanner::new(CORRIDOR_ORIGIN, CORRIDOR_GOAL, 1_000_000);
            pending(&mut planner, &Corridor, 2 + 9);
            planner
        }

        /// One corridor expansion is one pop and eight probes. It reads eight
        /// tiles, two edges and two step costs, so it fills all three memo
        /// maps, and it adds one record and one frontier node to trees that
        /// have room for them: the memo is the only thing it can allocate.
        const EXPANSION: u32 = 9;

        #[test]
        fn a_poll_allocates_no_memo_maps_after_the_poll_before_it() {
            let mut planner = corridor_planner();
            pending(&mut planner, &Corridor, EXPANSION);
            let (requests, ()) = heap_requests(|| pending(&mut planner, &Corridor, EXPANSION));
            assert_eq!(
                requests, 0,
                "the second expansion poll read the same amount of ground as the first, \
                 yet made {requests} heap requests for its memo maps"
            );
            assert_eq!(planner.expansions(), 2);
        }

        #[test]
        fn a_fresh_planner_allocates_no_memo_maps_on_a_warm_thread() {
            // Most polls of a tick are the first poll of a new order, so the
            // memo has to outlive the planner that warmed it.
            let mut warm = corridor_planner();
            pending(&mut warm, &Corridor, EXPANSION);
            let mut fresh = RoutePlanner::new(CORRIDOR_ORIGIN, CORRIDOR_GOAL, 1_000_000);
            // Two units validate the two ends: two tiles read, no search yet.
            let (requests, ()) = heap_requests(|| pending(&mut fresh, &Corridor, 2));
            assert_eq!(
                requests, 0,
                "the first poll of a new planner made {requests} heap requests for its memo \
                 after another planner's poll on this thread"
            );
        }

        /// Open ground cut by a wall whose only gap is far to the north; the
        /// search floods its own side for far more than one fast poll.
        struct Walled;
        const WALLED_ORIGIN: TileCoord = TileCoord::new(126, 128);
        const WALLED_GOAL: TileCoord = TileCoord::new(140, 128);

        impl RouteTerrain for Walled {
            fn route_walkable(
                &self,
                _: &ResourceOverlay,
                tile: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<bool> {
                Ok((0..256).contains(&tile.x)
                    && (0..256).contains(&tile.y)
                    && (tile.x != 128 || tile.y < 5))
            }
            fn route_edge_clear(
                &self,
                _: TileCoord,
                _: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<bool> {
                Ok(true)
            }
            fn route_step_cost(
                &self,
                from: TileCoord,
                to: TileCoord,
                _: &dyn Fn() -> bool,
            ) -> Answer<u32> {
                Ok(step_cost(from, to))
            }
        }

        /// The simulation's fast poll (`FAST_ROUTE_WORK`).
        const FAST_POLL: u32 = 8_192;
        const SLICES: u32 = 64;

        /// Heap requests of a new planner spending one fast poll of work in
        /// `polls` equal polls, and the hash of the planner it leaves.
        fn sliced(polls: u32) -> (u64, blake3::Hash) {
            let mut planner = RoutePlanner::new(WALLED_ORIGIN, WALLED_GOAL, 1_000_000);
            let (requests, ()) = heap_requests(|| {
                for _ in 0..polls {
                    pending(&mut planner, &Walled, FAST_POLL / polls);
                }
            });
            let mut hash = blake3::Hasher::new();
            planner.update_hash(&mut hash);
            (requests, hash.finalize())
        }

        #[test]
        fn heap_requests_of_a_search_do_not_depend_on_how_its_work_is_sliced() {
            // The frontier, the records and the pocket proof do the same
            // operations however the work is cut into polls; only a memo
            // rebuilt for every poll makes the number of polls matter.
            let _ = sliced(1);
            let (whole, whole_state) = sliced(1);
            let (cut, cut_state) = sliced(SLICES);
            assert_eq!(whole_state, cut_state, "the slicing changed the search");
            assert_eq!(
                cut, whole,
                "{FAST_POLL} work units made {whole} heap requests in one poll and {cut} in \
                 {SLICES} polls: every poll regrew its memo maps"
            );
        }
    }
}
