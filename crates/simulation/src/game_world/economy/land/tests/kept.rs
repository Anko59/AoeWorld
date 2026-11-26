use super::*;

const SIDE: i32 = 400;
const VIEWER: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

/// Discs of `radius` on a grid of eight a band: none touches another, shares a
/// row with another band or leaves the map.
fn grid(ids: impl IntoIterator<Item = i32>, radius: i32) -> Sources {
    let disc = |n: i32| {
        let centre = TileCoord::new(40 + n % 8 * 40, 40 + n / 8 * 40);
        (PlayerId((n % 3) as u16), centre, radius)
    };
    ids.into_iter().map(disc).collect()
}

/// Whether a disc only one of the two lists has covers row `y`.
fn touched(old: &[(PlayerId, TileCoord, i32)], new: &[(PlayerId, TileCoord, i32)], y: i32) -> bool {
    let covers = |(_, centre, radius): &&(PlayerId, TileCoord, i32)| {
        (centre.y - radius..=centre.y + radius).contains(&y)
    };
    old.iter().filter(covers).any(|s| !new.contains(s))
        || new.iter().filter(covers).any(|s| !old.contains(s))
}

#[test]
fn more_than_a_quarter_of_the_source_width_changed_means_one_whole_drawing() {
    // Width of the sources removed and added, against all of the new ones:
    // these are what a whole drawing sweeps.
    let mixed = |farms: std::ops::Range<i32>, castles: std::ops::Range<i32>| {
        [grid(farms, 2), grid(castles, 16)].concat()
    };
    let cases = [
        ("4 of 16 added", grid(0..12, 8), grid(0..16, 8), 0),
        ("5 of 17 added", grid(0..12, 8), grid(0..17, 8), 1),
        ("4 lost, 16 left", grid(0..20, 8), grid(0..16, 8), 0),
        ("5 lost, 16 left", grid(0..21, 8), grid(0..16, 8), 1),
        // A source that moves or changes hands is one removed and one added.
        (
            "2 of 16 moved",
            grid(0..16, 8),
            grid((2..16).chain(32..34), 8),
            0,
        ),
        (
            "3 of 16 moved",
            grid(0..16, 8),
            grid((3..16).chain(32..35), 8),
            1,
        ),
        // Width, not the number of sources.
        ("Castle, 16 Farms", grid(0..16, 2), mixed(0..16, 16..17), 1),
        ("4 Farms, 8 Castles", grid(0..8, 16), mixed(8..12, 0..8), 0),
        // The first sight of a realm, and the loss of one.
        ("30 seen at once", grid(0..4, 8), grid(0..34, 8), 1),
        ("30 lost at once", grid(0..34, 8), grid(0..4, 8), 1),
    ];
    for (name, old, new, whole) in cases {
        let world = land_world(SIDE);
        drop(world.known_land(VIEWER, old.clone()));
        assert_eq!(world.land_full_rasters(), 1, "{name}: the first drawing");
        let before = row_buffers(&world, VIEWER);
        let rows = world.known_land(VIEWER, new.clone());
        assert_eq!(*rows, reference_raster(&new, SIDE, SIDE), "{name}");
        assert_eq!(world.land_full_rasters() - 1, whole, "{name}");
        assert_eq!(world.land_known_rasters(), 2, "{name}");
        if whole == 0 {
            // Not whole: only the rows a changed source covers are drawn.
            let after = row_buffers(&world, VIEWER);
            let mut untouched = 0;
            for (y, buffer) in before.iter().filter(|(y, _)| !touched(&old, &new, **y)) {
                assert_eq!(after.get(y), Some(buffer), "{name}: row {y}");
                untouched += 1;
            }
            assert!(untouched > 0, "{name}");
        }
        // The same sources again draw nothing.
        assert!(Arc::ptr_eq(&rows, &world.known_land(VIEWER, new)));
        assert_eq!(world.land_full_rasters() - 1, whole, "{name}");
        assert_eq!(world.land_known_rasters(), 2, "{name}");
    }
}

#[test]
fn a_realm_seen_at_once_and_fog_off_then_on_cost_at_most_one_whole_drawing() {
    let mut world = land_world(160);
    let build = |world: &mut GameWorld, player, kind, (x, y): (i32, i32), side| {
        let rect = aoe_core::TileRect::from_xywh(x, y, side, side);
        world.insert_building(player, kind, rect, true).unwrap();
    };
    let second = |world: &mut GameWorld| {
        for _ in 0..20 {
            world.advance();
        }
    };
    build(&mut world, VIEWER, BuildingKind::TownCenter, (20, 20), 4);
    // A quarter of Outposts that fits in one Scout's sight.
    for n in 0..16 {
        let tile = (90 + n % 4 * 2, 90 + n / 4 * 2);
        build(&mut world, RIVAL, BuildingKind::Outpost, tile, 1);
    }
    let report = world.land_control(VIEWER);
    assert!(report.shapes.iter().all(|span| span.player == VIEWER));
    // A report on its own: the shared refresh is made and counted before.
    let drawn = |world: &GameWorld| {
        let fresh = report_from_nothing(world, VIEWER);
        world.held_land(VIEWER);
        let before = (world.land_full_rasters(), world.land_known_rasters());
        let report = world.land_control(VIEWER);
        assert_eq!(report, fresh);
        let full = world.land_full_rasters() - before.0;
        (report, full, world.land_known_rasters() - before.1)
    };
    let at = aoe_core::WorldPosition::from_tile_center(TileCoord::new(93, 93)).unwrap();
    let scout = world.spawn_unit(VIEWER, at).unwrap();
    world.advance();
    world.remove_unit(scout);
    second(&mut world);
    // The four around the Scout at least, against one Town Center.
    assert!(world.remembered_land_buildings(VIEWER).count() >= 4);
    let (report, full, known) = drawn(&world);
    assert!(report.shapes.iter().any(|span| span.player == RIVAL));
    assert_eq!((full, known), (1, 1), "first sight of a realm");
    // Fog off then on: what was remembered is what was drawn.
    world.set_fog_disabled(true);
    world.land_control(VIEWER);
    second(&mut world);
    world.land_control(VIEWER);
    world.set_fog_disabled(false);
    second(&mut world);
    let (_, full, known) = drawn(&world);
    assert!(
        full <= known && known <= 1,
        "fog off then on: {full}, {known}"
    );
    // As much built while the fog was off as was held before.
    world.set_fog_disabled(true);
    for n in 0..25 {
        let tile = (20 + n % 5 * 14, 60 + n / 5 * 14);
        build(&mut world, VIEWER, BuildingKind::Outpost, tile, 1);
    }
    second(&mut world);
    world.land_control(VIEWER);
    world.set_fog_disabled(false);
    second(&mut world);
    let (_, full, known) = drawn(&world);
    assert_eq!((full, known), (1, 1), "a realm built in the meantime");
}

#[test]
fn racing_reports_of_one_viewer_are_equal_and_share_the_first_stored() {
    let mut world = land_world(160);
    let outpost = |world: &mut GameWorld, x: i32, y: i32| {
        let rect = aoe_core::TileRect::from_xywh(x, y, 1, 1);
        world
            .insert_building(VIEWER, BuildingKind::Outpost, rect, true)
            .unwrap()
    };
    for n in 0..20 {
        outpost(&mut world, 10 + n % 10 * 14, 20 + n / 10 * 14);
    }
    let (mut few, mut many) = (Vec::new(), Vec::new());
    for round in 0..16_u64 {
        // One source more, many more, one less, many less.
        match round % 4 {
            0 => few.push(outpost(&mut world, 10, 130)),
            1 => many.extend((0..30).map(|n| {
                let (x, y) = (10 + n % 10 * 14, 60 + n / 10 * 14);
                outpost(&mut world, x, y)
            })),
            2 => few.drain(..).for_each(|id| world.remove_building(id)),
            _ => many.drain(..).for_each(|id| world.remove_building(id)),
        }
        world.tick = aoe_core::Tick(20 * (round + 1));
        let fresh = report_from_nothing(&world, VIEWER);
        let drawn = world.land_known_rasters();
        let start = std::sync::Barrier::new(8);
        let (world, start) = (&world, &start);
        let reports = std::thread::scope(|scope| {
            let race = || {
                start.wait();
                world.land_control(VIEWER)
            };
            let racers = (0..8).map(|_| scope.spawn(race)).collect::<Vec<_>>();
            racers
                .into_iter()
                .map(|racer| racer.join().unwrap())
                .collect::<Vec<_>>()
        });
        for report in &reports {
            assert_eq!(*report, fresh, "round {round}");
            assert!(Arc::ptr_eq(report, &reports[0]), "round {round}");
        }
        assert!(world.land_known_rasters() > drawn, "round {round} drew");
        let cache = world.economy.land.lock().unwrap();
        let (sources, rows) = &cache.known[&VIEWER];
        let expected = reference_raster(sources, 160, 160);
        assert_eq!(**rows, expected, "round {round}");
    }
}

#[test]
fn a_drawing_still_read_is_never_edited_and_racing_drawings_keep_to_their_sources() {
    let world = land_world(SIDE);
    let draw = |sources: &Sources| world.known_land(VIEWER, sources.clone());
    let expect = |sources: &Sources| reference_raster(sources, SIDE, SIDE);
    // A thirteenth more: edited, not drawn whole.
    let (few, more) = (grid(0..12, 8), grid(0..13, 8));
    let held = draw(&few);
    assert!(Arc::ptr_eq(&held, &draw(&few)), "one drawing is shared");
    let edited = draw(&more);
    assert_eq!(world.land_full_rasters(), 1);
    assert!(!Arc::ptr_eq(&held, &edited), "the reader keeps its own");
    assert_eq!(*held, expect(&few));
    assert_eq!(*edited, expect(&more));
    // Threads ask for small and large changes in turn, each holding the last
    // drawing it was given across the next one.
    let asked = [few, more, grid(0..40, 8)].map(|sources| {
        let rows = expect(&sources);
        (sources, rows)
    });
    let (draw, asked) = (&draw, &asked);
    std::thread::scope(|scope| {
        for thread in 0..6 {
            scope.spawn(move || {
                let mut last = None;
                for turn in 0..60 {
                    let (sources, rows) = &asked[(thread + turn) % asked.len()];
                    let drawn = draw(sources);
                    assert_eq!(*drawn, *rows, "thread {thread}, turn {turn}");
                    if let Some((older, rows)) = last.replace((drawn, rows)) {
                        assert_eq!(*older, *rows, "thread {thread}, turn {turn}");
                    }
                }
            });
        }
    });
    let cache = world.economy.land.lock().unwrap();
    let (sources, rows) = &cache.known[&VIEWER];
    assert_eq!(**rows, expect(sources));
}
