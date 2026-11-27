use super::*;

// What the watch counts, fed by hand: a line across a neck that runs west to east, between the
// tiles (10, 5) and (10, 15): ground from y = 6 to 14, nine tiles wide.
const TILE_SUBUNITS: i32 = 1024;

fn neck() -> Choke {
    Choke {
        name: "synthetic",
        saddle: (10, 10),
        ends: ((10, 5), (10, 15)),
        width_tiles: 9,
        length_tiles: 4,
        flow: (1, 0),
        near: (2, 10),
        far: (18, 10),
        sides: "walls",
        why: "a test",
    }
}

/// A position in tiles, as the engine keeps it: subunits, the centre of the tile.
fn at(x: i32, y: i32) -> (i32, i32) {
    (x * TILE_SUBUNITS + 512, y * TILE_SUBUNITS + 512)
}

fn walker(id: u32, seat: u16, position: (i32, i32), stepped: bool) -> Walker {
    Walker {
        id,
        seat,
        position,
        stepped,
        goal: None,
        alive: true,
    }
}

/// One tick: every walker seen, the sum of what they showed.
fn tick(watch: &mut ChokeWatch, walkers: &[Walker]) -> Moves {
    let mut moves = Moves::default();
    for &walker in walkers {
        watch.see(walker, &mut moves);
    }
    moves
}

#[test]
fn a_unit_that_steps_over_the_line_has_crossed_once_and_only_once() {
    let mut watch = ChokeWatch::new(&neck());
    assert_eq!(
        tick(&mut watch, &[walker(1, 0, at(7, 10), true)]).crossed,
        0
    );
    assert_eq!(
        tick(&mut watch, &[walker(1, 0, at(9, 10), true)]).crossed,
        0
    );
    let over = tick(&mut watch, &[walker(1, 0, at(11, 10), true)]);
    assert_eq!(
        over.crossed, 1,
        "the step from x 9 to x 11 cuts the line at x 10"
    );
    for x in [12, 9, 11, 13] {
        let again = tick(&mut watch, &[walker(1, 0, at(x, 10), true)]);
        assert_eq!(
            again.crossed, 0,
            "going back and forth is not another crossing"
        );
    }
    let totals = watch.totals();
    assert_eq!((totals.sent, totals.near_side), (1, 0));
}

#[test]
fn a_unit_going_around_the_end_of_the_line_has_not_crossed() {
    let mut watch = ChokeWatch::new(&neck());
    // North of the blocked tile at (10, 5): beyond the segment, the line's extension.
    tick(&mut watch, &[walker(1, 0, at(8, 2), true)]);
    let round = tick(&mut watch, &[walker(1, 0, at(12, 2), true)]);
    assert_eq!(round.crossed, 0);
    assert_eq!(
        watch.totals().near_side,
        1,
        "it is still on the near side of the neck"
    );
}

#[test]
fn a_unit_that_lands_on_the_line_is_judged_by_the_step_that_leaves_it() {
    let mut watch = ChokeWatch::new(&neck());
    tick(&mut watch, &[walker(1, 0, at(8, 10), true)]);
    // Exactly on the line (the tile centre of x = 10 is 512 subunits into the tile; the line is
    // drawn through the centres of the end tiles, so x = 10 tile centre is on it).
    let on = tick(&mut watch, &[walker(1, 0, at(10, 10), true)]);
    assert_eq!(on.crossed, 0, "on the line is neither side");
    let off_far = tick(&mut watch, &[walker(1, 0, at(12, 10), true)]);
    assert_eq!(off_far.crossed, 1);
    // And a unit that lands on the line and goes back has not crossed.
    let mut watch = ChokeWatch::new(&neck());
    tick(&mut watch, &[walker(2, 0, at(8, 10), true)]);
    tick(&mut watch, &[walker(2, 0, at(10, 10), true)]);
    assert_eq!(
        tick(&mut watch, &[walker(2, 0, at(8, 11), true)]).crossed,
        0
    );
}

#[test]
fn units_of_both_armies_cross_toward_each_other_and_the_dead_stop_counting() {
    let mut watch = ChokeWatch::new(&neck());
    let start = [walker(1, 0, at(8, 9), true), walker(2, 1, at(12, 11), true)];
    let moves = tick(&mut watch, &start);
    assert_eq!(moves.alive, [1, 1]);
    let over = tick(
        &mut watch,
        &[walker(1, 0, at(12, 9), true), walker(2, 1, at(8, 11), true)],
    );
    assert_eq!(
        over.crossed, 2,
        "each crossed from its own side to the other"
    );
    let mut dead = walker(2, 1, at(8, 11), false);
    dead.alive = false;
    let after = tick(&mut watch, &[walker(1, 0, at(13, 9), true), dead]);
    assert_eq!((after.alive, after.crossed), ([1, 0], 0));
    let totals = watch.totals();
    assert_eq!(
        (totals.sent, totals.sent_by_seat, totals.alive_by_seat),
        (2, [1, 1], [1, 0])
    );
}

#[test]
fn standing_still_inside_the_neck_is_counted_in_ticks_and_walking_resets_it() {
    let mut watch = ChokeWatch::new(&neck());
    // Inside: between the ends and within the reach (length 4 gives 3 tiles each way).
    let inside = at(11, 10);
    assert_eq!(
        tick(&mut watch, &[walker(1, 0, inside, true)]).still_peak,
        0
    );
    for expected in 1..=3 {
        let moves = tick(&mut watch, &[walker(1, 0, inside, false)]);
        assert_eq!(moves.still_peak, expected);
    }
    assert_eq!(
        tick(&mut watch, &[walker(1, 0, at(12, 10), true)]).still_peak,
        0
    );
    // Outside the neck (twelve tiles before the line) standing is waiting, not jamming.
    let before = at(-2, 10);
    for _ in 0..5 {
        assert_eq!(
            tick(&mut watch, &[walker(2, 0, before, false)]).still_peak,
            0
        );
    }
    // The longest of several units is the peak.
    let moves = tick(
        &mut watch,
        &[
            walker(3, 0, at(10, 8), false),
            walker(4, 0, at(10, 9), false),
        ],
    );
    assert_eq!(moves.still_peak, 1);
}

#[test]
fn the_census_counts_units_waiting_and_units_within_two_tiles_of_their_goal() {
    let mut watch = ChokeWatch::new(&neck());
    let sent = |id, x, goal: Option<(i32, i32)>| Walker {
        goal: goal.map(|(gx, gy)| at(gx, gy)),
        ..walker(id, 0, at(x, 10), true)
    };
    let walkers = [
        sent(1, 3, Some((18, 10))),
        sent(2, 17, Some((18, 10))),
        sent(3, 12, Some((18, 10))),
    ];
    tick(&mut watch, &walkers);
    let totals = watch.totals();
    assert_eq!(totals.sent, 3);
    assert_eq!(
        totals.at_goal, 1,
        "only unit 2 is within two tiles of (18, 10)"
    );
    assert_eq!(totals.near_side, 3, "nobody has cut the line yet");
    // A unit that arrived has no order any more, but keeps the goal it was given.
    let arrived = Walker {
        goal: None,
        ..walker(2, 0, at(18, 10), false)
    };
    tick(&mut watch, &[arrived]);
    assert_eq!(watch.totals().at_goal, 1);
}

#[test]
fn a_blow_makes_fighters_of_the_units_that_took_part_once_and_ignores_strangers() {
    let mut watch = ChokeWatch::new(&neck());
    tick(
        &mut watch,
        &[
            walker(1, 0, at(8, 9), false),
            walker(2, 1, at(12, 9), false),
            walker(3, 0, at(5, 9), false),
        ],
    );
    assert_eq!(watch.blows([(1, 2)]), 2);
    assert_eq!(watch.blows([(1, 2), (2, 1)]), 0, "the same two again");
    assert_eq!(
        watch.blows([(99, 1), (3, 77)]),
        1,
        "an unknown id fights no one"
    );
    let totals = watch.totals();
    assert_eq!((totals.fought, totals.sent), (3, 3));
}

/// A map drawn in code: two rooms of forty by forty tiles joined by a door three tiles wide and
/// six long, in a square of a hundred.
struct Rooms {
    door_open: bool,
}

impl Ground for Rooms {
    fn side(&self) -> i32 {
        100
    }

    fn walkable(&self, (x, y): Tile) -> bool {
        let room = |from: i32| (from..from + 40).contains(&x) && (2..42).contains(&y);
        let door = (42..48).contains(&x) && (20..23).contains(&y);
        room(2) || room(48) || (door && self.door_open)
    }
}

fn rooms() -> (Grid, Vec<u16>) {
    let grid = Grid::of(&Rooms { door_open: true });
    let clear = clearance(&grid);
    (grid, clear)
}

#[test]
fn clearance_is_the_distance_to_the_nearest_wall_in_thirds_of_a_tile() {
    let (grid, clear) = rooms();
    let at = |x: i32, y: i32| clear[(y * grid.side() + x) as usize];
    assert_eq!(at(0, 0), 0, "a wall has none");
    assert_eq!(at(2, 5), 3, "the tile beside a wall is one tile from it");
    assert_eq!(
        at(45, 21),
        6,
        "the middle of the door is two tiles from either wall"
    );
    assert_eq!(at(45, 20), 3, "its edge is one");
    assert!(
        at(12, 12) >= 27,
        "the middle of a room is nine tiles from a wall"
    );
}

#[test]
fn the_saddle_between_two_rooms_is_in_their_door_and_the_door_is_measured() {
    let (grid, clear) = rooms();
    let found = saddles(&grid, &clear, 6, 100);
    let saddle = found.first().expect("the door is a saddle");
    assert_eq!(saddle.clearance, 6);
    assert!(
        (42..48).contains(&saddle.tile.0) && saddle.tile.1 == 21,
        "{saddle:?}"
    );
    assert!(
        (100..=1600).contains(&saddle.sides.0) && (100..=1600).contains(&saddle.sides.1),
        "each room is the ground on its side: {saddle:?}"
    );
    assert_eq!(found.len(), 1, "one door, one saddle");
    let neck = corridor(&|tile| grid.walkable(tile), saddle.tile).expect("a neck");
    assert_eq!(neck.width, 3);
    assert_eq!(neck.length, 6, "the door is six cross-sections long");
    assert_eq!(neck.flow, (1, 0));
    assert_eq!(neck.ends, ((saddle.tile.0, 19), (saddle.tile.0, 23)));
}

#[test]
fn a_neck_narrower_than_asked_or_between_small_stretches_is_not_a_saddle() {
    let (grid, clear) = rooms();
    assert!(
        saddles(&grid, &clear, 3, 100).is_empty(),
        "the door is wider than one tile"
    );
    assert!(
        saddles(&grid, &clear, 6, 100_000).is_empty(),
        "a room does not have that much ground"
    );
}

/// Two hundred tiles of open ground: nothing blocks within sixty-four tiles of its middle.
#[test]
fn open_ground_has_no_neck() {
    let open = |(x, y): Tile| (0..200).contains(&x) && (0..200).contains(&y);
    assert_eq!(corridor(&open, (100, 100)), None);
    // A tile that blocks has no neck either.
    assert_eq!(corridor(&open, (-5, 100)), None);
}

#[test]
fn closing_the_neck_closes_the_way_between_the_anchors() {
    let (grid, clear) = rooms();
    let saddle = saddles(&grid, &clear, 6, 100)[0].tile;
    let neck = corridor(&|tile| grid.walkable(tile), saddle).expect("a neck");
    let (near, far) = anchors(&grid, &clear, &neck, 10).expect("anchors");
    assert!(near.0 < 42 && far.0 > 47, "{near:?} {far:?}");
    assert!(grid.walkable(near) && grid.walkable(far));
    let through = steps(&grid, near, far, &[]).expect("a way");
    assert!(
        through >= (far.0 - near.0) as u32,
        "no shorter than the straight line"
    );
    let closed = [(saddle.0, 20), (saddle.0, 21), (saddle.0, 22)];
    assert_eq!(
        steps(&grid, near, far, &closed),
        None,
        "the door was the only way"
    );
    let sealed = Grid::of(&Rooms { door_open: false });
    assert_eq!(steps(&sealed, near, far, &[]), None);
}

#[test]
fn a_neck_says_what_blocks_its_sides() {
    let (grid, _) = rooms();
    let neck = corridor(&|tile| grid.walkable(tile), (45, 21)).expect("a neck");
    // The synthetic ground draws every blocked tile as `#`. The neck is six long, so nine
    // cross-sections are read (four either way); of the eighteen tiles at their ends the twelve
    // inside the door's walls block, and the six that fall in a room do not.
    assert_eq!(flanks(&Rooms { door_open: true }, &neck), "#x12");
}

#[test]
fn the_survey_lists_the_necks_an_army_has_to_use_with_a_picture_and_the_detail_asked_for() {
    // Two rooms joined by one door: closing it shuts the way, so the detour is `99`.
    let text = survey(&Rooms { door_open: true }, &[(45, 21)]);
    assert!(text.contains("1 kept"), "{text}");
    assert!(text.contains("#0 detour 99.00"), "{text}");
    assert!(text.contains("width 3 length 6"), "{text}");
    assert!(text.contains("None with the neck closed"), "{text}");
    assert!(text.contains("DETAIL (45, 21)"), "{text}");
    assert!(text.contains("flanks #x12"), "{text}");
    assert!(text.contains('S'), "a picture marks the saddle");
    // With the door shut there is nothing to survey.
    let sealed = survey(&Rooms { door_open: false }, &[]);
    assert!(sealed.contains("0 kept"), "{sealed}");
}
