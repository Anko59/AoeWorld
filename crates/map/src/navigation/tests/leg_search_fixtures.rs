//! The leg-search pins on the map fixtures the planner tests already use:
//! open ground, a wall with and without a gateway, far orders whose legs are
//! provisional, bridges, and a far goal sealed behind buildings.
use super::*;

fn tile(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn flat(
    width: i32,
    wall_x: i32,
    gap_y: i32,
    prepared: bool,
) -> (MapChunkGenerator, ResourceOverlay) {
    let terrain = flat_terrain_with_water_wall(width, wall_x, gap_y);
    let overlay = ResourceOverlay::default();
    if prepared {
        terrain.prepare_navigation(&overlay, &|| false).unwrap();
    }
    (terrain, overlay)
}

fn bridge(overlay: &mut ResourceOverlay, from: TileCoord, to: TileCoord) {
    let span = aoe_core::BridgeSpan::new(from, to).expect("bridge span");
    overlay.add_bridge(span).expect("bridge");
}

#[test]
fn open_ground_is_one_exact_leg() {
    let (origin, destination) = (tile(5, 7), tile(41, 29));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(128, -1, -1, false);
        let mut planner = RoutePlanner::new(origin, destination, 65_536);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(run.cost, heuristic(origin, destination));
        assert_eq!(run.tiles.last(), Some(&destination));
        run
    };
    let pinned = [
        "budget=8192 calls=3 end=complete work=399 expansions=36 peak=388 tiles=37 last=41,29 near=- cost=46192 route=cfac99f8011b479e trace=30f921e72e951a0b",
        "budget=127 calls=6 end=complete work=399 expansions=36 peak=388 tiles=37 last=41,29 near=- cost=46192 route=cfac99f8011b479e trace=2c0785e0107488a2",
        "budget=1 calls=401 end=complete work=399 expansions=36 peak=388 tiles=37 last=41,29 near=- cost=46192 route=cfac99f8011b479e trace=dee0a7a0b830630b",
    ];
    let runs = [(FAST_POLL, false), (127, false), (1, false)];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn a_far_order_over_open_ground_walks_provisional_legs() {
    let (origin, destination) = (tile(1, 1), tile(700, 300));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(4_096, -1, -1, false);
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(run.tiles.last(), Some(&destination));
        run
    };
    // Lookahead plans the next leg before the walk needs it: the same legs,
    // emitted in a different order of calls.
    let pinned = [
        "budget=8192 calls=36 end=complete work=7709 expansions=699 peak=492 tiles=700 last=700,300 near=- cost=842552 route=4dfd0e74f5062c73 trace=350cc2df9f1340c7",
        "budget=127 calls=87 end=complete work=7709 expansions=699 peak=492 tiles=700 last=700,300 near=- cost=842552 route=4dfd0e74f5062c73 trace=9a01990aa9c83409",
        "budget=8192+ahead calls=40 end=complete work=7709 expansions=699 peak=598 tiles=700 last=700,300 near=- cost=842552 route=4dfd0e74f5062c73 trace=4f33341c77a29c8c",
        "budget=127+ahead calls=87 end=complete work=7709 expansions=699 peak=492 tiles=700 last=700,300 near=- cost=842552 route=4dfd0e74f5062c73 trace=3e55ce91e69ae1b2",
    ];
    let runs = [
        (FAST_POLL, false),
        (127, false),
        (FAST_POLL, true),
        (127, true),
    ];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn a_far_order_with_prepared_navigation_follows_portal_guidance() {
    let (origin, destination) = (tile(1, 1), tile(700, 300));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(4_096, -1, -1, true);
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(run.tiles.last(), Some(&destination));
        run
    };
    // With a prepared atlas the legs follow portal waypoints once the abstract
    // search has answered, and how soon it answers depends on the budget. The
    // guided routes are far longer than the direct one: that is the planner's
    // current output here, pinned so that guided legs are covered too.
    let pinned = [
        "budget=8192 calls=104 end=complete work=72041 expansions=1623 peak=2821 tiles=1624 last=700,300 near=- cost=1970200 route=65f0b7b53da9ffdd trace=0cb20f7f6fd86ecd",
        "budget=8192+ahead calls=151 end=complete work=69925 expansions=1431 peak=2807 tiles=1432 last=700,300 near=- cost=1773592 route=83a4d05b4d65380a trace=402e61de1863b831",
        "budget=1021+ahead calls=59 end=complete work=23057 expansions=699 peak=1223 tiles=700 last=700,300 near=- cost=842552 route=4dfd0e74f5062c73 trace=d423b02dfbbea439",
    ];
    let runs = [(FAST_POLL, false), (FAST_POLL, true), (1_021, true)];
    assert_pins(&lines(&runs, scenario), &pinned);
}

#[test]
fn a_wall_is_passed_through_its_only_gateway() {
    let (origin, destination) = (tile(1, 32), tile(62, 32));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(64, 32, 5, false);
        let mut planner = RoutePlanner::new(origin, destination, 100_000);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(run.tiles.last(), Some(&destination));
        assert!(run.tiles.contains(&tile(32, 5)));
        run
    };
    let pinned = [
        "budget=4096 calls=9 end=complete work=22373 expansions=1417 peak=2022 tiles=62 last=62,32 near=- cost=85360 route=475800c65a3c4224 trace=fe65953a3a0d14da",
        "budget=127 calls=179 end=complete work=22373 expansions=1417 peak=2022 tiles=62 last=62,32 near=- cost=85360 route=475800c65a3c4224 trace=1f309825454cc01d",
    ];
    assert_pins(&lines(&[(4_096, false), (127, false)], scenario), &pinned);
}

#[test]
fn a_wall_without_a_gateway_ends_at_the_certified_nearest_tile() {
    let (origin, destination) = (tile(1, 32), tile(62, 32));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(64, 32, -1, false);
        let mut planner = RoutePlanner::new(origin, destination, 100_000);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(planner.certified_destination(), Some(tile(31, 32)));
        run
    };
    let pinned = [
        "budget=4096 calls=17 end=unreachable work=65171 expansions=1603 peak=3876 tiles=31 last=31,32 near=31,32 cost=30720 route=7fc0e82047e1c6a9 trace=f0b26b3a4b1c90d7",
        "budget=127 calls=515 end=unreachable work=65171 expansions=1603 peak=3876 tiles=31 last=31,32 near=31,32 cost=30720 route=7fc0e82047e1c6a9 trace=ad0c6c3d00fb8920",
    ];
    assert_pins(&lines(&[(4_096, false), (127, false)], scenario), &pinned);
}

#[test]
fn a_long_provisional_leg_floods_behind_a_wall_before_its_gateway() {
    let (origin, destination) = (tile(8, 200), tile(250, 200));
    let scenario = |budget, lookahead| {
        let (terrain, overlay) = flat(256, 32, 30, false);
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(run.tiles.last(), Some(&destination));
        assert!(
            run.tiles.iter().any(|tile| tile.y < 140),
            "used the gateway"
        );
        run
    };
    let pinned = [
        "budget=16384 calls=36 end=complete work=129817 expansions=9051 peak=7591 tiles=340 last=250,200 near=- cost=388264 route=ff287c0a2dae156a trace=6305594b6409c443",
        "budget=1021 calls=151 end=complete work=129817 expansions=9051 peak=7591 tiles=340 last=250,200 near=- cost=388264 route=ff287c0a2dae156a trace=9a2a240cf9fccfe6",
    ];
    assert_pins(
        &lines(&[(16_384, false), (1_021, false)], scenario),
        &pinned,
    );
}

#[test]
fn bridges_carry_local_legs_over_water() {
    let scenario = |origin: TileCoord, destination: TileCoord| {
        move |budget, lookahead| {
            let (terrain, mut overlay) = flat(64, 32, -1, false);
            bridge(&mut overlay, tile(31, 20), tile(33, 22));
            bridge(&mut overlay, tile(31, 40), tile(33, 40));
            terrain.prepare_navigation(&overlay, &|| false).unwrap();
            let mut planner = RoutePlanner::new(origin, destination, 65_536);
            let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
            assert_eq!(run.tiles.last(), Some(&destination));
            run
        }
    };
    let runs = [(2_048, false), (1, false)];
    let diagonal = scenario(tile(28, 20), tile(36, 22));
    let over_the_deck = [
        (28, 20),
        (29, 20),
        (30, 20),
        (31, 20),
        (32, 21),
        (33, 22),
        (34, 22),
        (35, 22),
        (36, 22),
    ];
    assert_eq!(
        diagonal(2_048, false).tiles,
        over_the_deck.map(|(x, y)| tile(x, y))
    );
    let straight = scenario(tile(29, 40), tile(35, 40));
    assert!(straight(2_048, false).tiles.iter().all(|tile| tile.y == 40));
    let observed = [
        lines(&runs, diagonal),
        lines(&runs, scenario(tile(36, 22), tile(28, 20))),
        lines(&runs, straight),
    ]
    .concat();
    let pinned = [
        "budget=2048 calls=2 end=complete work=136 expansions=13 peak=84 tiles=9 last=36,22 near=- cost=9040 route=e5adbb16e5fadf84 trace=0fff9a5a78ba842d",
        "budget=1 calls=137 end=complete work=136 expansions=13 peak=84 tiles=9 last=36,22 near=- cost=9040 route=e5adbb16e5fadf84 trace=3a82c3ab97363594",
        "budget=2048 calls=2 end=complete work=136 expansions=13 peak=84 tiles=9 last=28,20 near=- cost=9040 route=eb9c6ad5c68cfd9b trace=a216fa8b4d3759d7",
        "budget=1 calls=137 end=complete work=136 expansions=13 peak=84 tiles=9 last=28,20 near=- cost=9040 route=eb9c6ad5c68cfd9b trace=a91d7c08ab9b824d",
        "budget=2048 calls=2 end=complete work=69 expansions=6 peak=56 tiles=7 last=35,40 near=- cost=6144 route=5f3f49ce05d99de5 trace=39f13c8e54f7dd4c",
        "budget=1 calls=70 end=complete work=69 expansions=6 peak=56 tiles=7 last=35,40 near=- cost=6144 route=5f3f49ce05d99de5 trace=c1d4ec82cf9478ec",
    ];
    assert_pins(&observed, &pinned);
}

#[test]
fn a_bridge_carries_provisional_legs_over_water() {
    // Far orders: every leg is provisional, so each probe also checks the
    // way back, over the deck as over land. Not pinned line by line: the
    // route crosses the only deck and does not depend on the slicing.
    let scenario = |origin: TileCoord, destination: TileCoord| {
        move |budget, lookahead| {
            let (terrain, mut overlay) = flat(256, 32, -1, false);
            let wet: Vec<i32> = (100..160)
                .filter(|x| !walkable(&terrain, tile(*x, 100)))
                .collect();
            assert_eq!(wet, [126, 127, 128, 129], "the fixture's wall");
            bridge(&mut overlay, tile(125, 100), tile(130, 100));
            let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
            let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
            assert_eq!(run.tiles.last(), Some(&destination));
            for x in wet {
                assert!(run.tiles.contains(&tile(x, 100)), "crossed the deck");
            }
            run
        }
    };
    let (west, east) = (tile(40, 100), tile(215, 100));
    for (origin, destination) in [(west, east), (east, west)] {
        let order = scenario(origin, destination);
        let (whole, cut) = (order(FAST_POLL, false), order(127, false));
        assert_eq!(whole.tiles, cut.tiles, "slicing changed the route");
        assert_eq!(whole.cost, cut.cost);
    }
}

#[test]
fn a_far_sealed_goal_is_approached_under_a_pinned_collision_snapshot() {
    let (origin, destination) = (tile(10, 512), tile(512, 512));
    let scenario = |budget, lookahead| {
        let (terrain, mut overlay) = flat(4_096, -1, -1, false);
        let ring = [
            (510, 510, 5, 1),
            (510, 514, 5, 1),
            (510, 511, 1, 3),
            (514, 511, 1, 3),
        ];
        for (x, y, width, height) in ring {
            overlay
                .add_footprint(aoe_core::TileRect::from_xywh(x, y, width, height))
                .unwrap();
        }
        terrain.prepare_navigation(&overlay, &|| false).unwrap();
        let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
        let run = drive(&mut planner, &terrain, &overlay, budget, lookahead);
        assert_eq!(planner.certified_destination(), Some(tile(512, 509)));
        run
    };
    // The endpoint proof pins its own collision snapshot, which the search
    // then checks beside the live overlay. The two budgets end at the same
    // tile by different routes: the planner's current output.
    let pinned = [
        "budget=8192 calls=22 end=unreachable work=42456 expansions=502 peak=2389 tiles=503 last=512,509 near=512,509 cost=515320 route=c3845b3477ea1b29 trace=6449f4fe14ebc110",
        "budget=127 calls=290 end=unreachable work=34768 expansions=1810 peak=1898 tiles=503 last=512,509 near=512,509 cost=515320 route=c43e9cf67242896e trace=feac409a92c84601",
    ];
    assert_pins(
        &lines(&[(FAST_POLL, false), (127, false)], scenario),
        &pinned,
    );
}
