use super::place::{fill, shuffled};
use super::*;
use crate::game_score::choke::{Grid, MapGround, anchors, clearance};
use crate::game_score::counters::Recorder;
use crate::game_score::scenarios::Span;
use crate::game_score::windows::{self, Record, WindowReport};
use aoe_simulation::terrain_fixtures::measurement_world;
use std::{collections::BTreeSet, time::Duration};

/// A map of sixty tiles a side with a wall across it at x = 20, and a gap five tiles wide in the
/// wall at y = 10 to 14.
struct Wall;

impl Ground for Wall {
    fn side(&self) -> i32 {
        60
    }

    fn walkable(&self, (x, y): Tile) -> bool {
        (0..60).contains(&x) && (0..60).contains(&y) && (x != 20 || (10..15).contains(&y))
    }
}

fn gap() -> Choke {
    Choke {
        name: "gap",
        saddle: (20, 12),
        ends: ((20, 9), (20, 15)),
        width_tiles: 5,
        length_tiles: 1,
        flow: (1, 0),
        near: (10, 12),
        far: (30, 12),
        sides: "a wall",
        why: "a test",
    }
}

#[test]
fn a_choke_recorded_as_the_ground_measures_it_is_verified() {
    assert_eq!(verify(&Wall, &gap()), Ok(()));
    assert_eq!(
        gap().across(),
        vec![(20, 10), (20, 11), (20, 12), (20, 13), (20, 14)]
    );
}

#[test]
fn a_record_that_differs_from_the_ground_is_refused_with_what_the_ground_says() {
    let narrower = Choke {
        width_tiles: 4,
        ..gap()
    };
    let error = verify(&Wall, &narrower).expect_err("the gap is five wide");
    assert!(
        error.contains("recorded as") && error.contains("gap") && error.contains("5"),
        "{error}"
    );
    let on_the_wall = Choke {
        saddle: (20, 5),
        ..gap()
    };
    let error = verify(&Wall, &on_the_wall).expect_err("no neck on a wall");
    assert!(error.contains("no neck at (20, 5)"), "{error}");
    let walled_in = Choke {
        near: (20, 3),
        ..gap()
    };
    let error = verify(&Wall, &walled_in).expect_err("an anchor on the wall");
    assert!(error.contains("anchor (20, 3)"), "{error}");
    let too_close = Choke {
        near: (15, 12),
        ..gap()
    };
    let error = verify(&Wall, &too_close).expect_err("an anchor in the neck's reach");
    assert!(
        error.contains("anchor (15, 12)") && error.contains("tiles behind"),
        "{error}"
    );
    let wrong_side = Choke {
        far: (10, 12),
        ..gap()
    };
    assert!(
        verify(&Wall, &wrong_side).is_err(),
        "the far anchor is on the near side"
    );
}

#[test]
fn the_ground_around_an_anchor_is_filled_nearest_first_through_neighbours_only() {
    // Ten by ten tiles with a wall down x = 5: nothing beyond it can be reached.
    let open = |(x, y): Tile| (0..10).contains(&x) && (0..10).contains(&y) && x != 5;
    let places = fill(&open, (2, 5), 30, 40);
    assert_eq!(places.len(), 30);
    assert_eq!(places[0], (2, 5), "the anchor comes first");
    assert_eq!(
        places.iter().collect::<BTreeSet<_>>().len(),
        30,
        "no tile twice"
    );
    assert!(
        places.iter().all(|&(x, _)| x < 5),
        "the far side is out of reach"
    );
    let reach = |tile: Tile| (tile.0 - 2).abs() + (tile.1 - 5).abs();
    assert!(
        places
            .windows(2)
            .all(|pair| reach(pair[0]) <= reach(pair[1])),
        "{places:?}"
    );
}

#[test]
fn the_fill_stops_where_the_ground_ends_and_where_the_limit_is() {
    let room = |(x, y): Tile| (0..3).contains(&x) && (0..3).contains(&y);
    assert_eq!(fill(&room, (1, 1), 100, 40).len(), 9);
    let open = |_: Tile| true;
    let near = fill(&open, (0, 0), 1_000, 2);
    assert!(near.iter().all(|&(x, y)| x.abs() <= 2 && y.abs() <= 2));
    assert_eq!(near.len(), 25);
}

#[test]
fn a_seed_is_a_layout_and_every_layout_is_a_permutation() {
    let one = shuffled(50, 1);
    assert_eq!(one, shuffled(50, 1), "the same seed, the same layout");
    assert_ne!(one, shuffled(50, 2), "another seed, another layout");
    assert_eq!(one.iter().collect::<BTreeSet<_>>().len(), 50);
    assert!(one.iter().all(|&n| n < 50));
    assert!(shuffled(0, 1).is_empty());
    assert_eq!(shuffled(1, 7), vec![0]);
}

/// The fixture of three rivers, each crossed only by its own single-lane bridge, with its units
/// taken away: the choke is the first bridge, one tile wide and six long.
fn bridge_world() -> GameWorld {
    measurement_world("long-bridges-40")
}

/// The bridge's neck, measured on the world as the survey measures the real map, as a record.
fn bridge_choke(world: &GameWorld) -> Choke {
    let ground = MapGround::of(world).expect("a map");
    let saddle = (150, 100);
    let walkable = |tile: Tile| ground.walkable(tile);
    let neck = corridor(&walkable, saddle).expect("the bridge is a neck");
    let grid = Grid::of(&ground);
    let clear = clearance(&grid);
    let (near, far) = anchors(&grid, &clear, &neck, 40).expect("open ground on both banks");
    Choke {
        name: "the first bridge",
        saddle,
        ends: neck.ends,
        width_tiles: neck.width,
        length_tiles: neck.length,
        flow: neck.flow,
        near,
        far,
        sides: "river",
        why: "a test",
    }
}

fn played(world: &mut GameWorld, staging: &Staging, ticks: u64) -> WindowReport {
    let mut recorder = Recorder::new(staging.seats(), world.config().tick_hz);
    recorder.stage(staging.choke);
    world.set_damage_observation(true);
    recorder.start(world);
    for _ in 0..ticks {
        world.advance();
        let hits = world
            .take_damage_observations()
            .into_iter()
            .map(|hit| (hit.attacker.0, hit.target.0));
        recorder.blows(hits);
        recorder.tick(world, Duration::ZERO);
    }
    recorder.sample(world);
    let record = Record {
        rows: &recorder.rows,
        samples: &recorder.samples,
        events: &recorder.events,
    };
    let span = Span {
        id: "whole",
        from: 0,
        to: ticks,
    };
    windows::window(record, &span).expect("a window")
}

const MILITIA: &[Army] = &[Army {
    seat: 0,
    from: Side::Near,
    mix: &[(UnitKind::Militia, 40)],
}];

#[test]
fn an_army_is_staged_on_the_open_ground_of_one_bank_and_ordered_across() {
    let mut world = bridge_world();
    let choke: &'static Choke = Box::leak(Box::new(bridge_choke(&world)));
    assert_eq!((choke.width_tiles, choke.flow), (1, (1, 0)), "{choke:?}");
    let staging = Staging {
        map: "test",
        choke,
        armies: MILITIA,
        attack: false,
    };
    let staged = stage(&mut world, &staging, 1).expect("the army is staged");
    assert_eq!(staged.units, 40);
    assert_eq!(world.unit_count(), 40);
    assert_eq!(staged.orders.len(), 1);
    assert!(
        staged.orders[0].contains("move 40 units"),
        "{:?}",
        staged.orders
    );
    let behind_the_line = world
        .units()
        .all(|unit| unit.position.x <= (150 - MARGIN) * 1024 + 1023);
    assert!(
        behind_the_line,
        "the army starts on the near bank, clear of the neck"
    );
    let again = {
        let mut other = bridge_world();
        stage(&mut other, &staging, 1).expect("again")
    };
    assert_eq!(staged, again, "the same seed stages the same orders");
}

#[test]
fn the_march_over_a_one_tile_bridge_is_counted_unit_by_unit() {
    let mut world = bridge_world();
    let choke: &'static Choke = Box::leak(Box::new(bridge_choke(&world)));
    let staging = Staging {
        map: "test",
        choke,
        armies: MILITIA,
        attack: false,
    };
    stage(&mut world, &staging, 1).expect("staged");
    let window = played(&mut world, &staging, 2_500);
    let crossing = window.service.choke.expect("a staged scenario has a choke");
    assert_eq!(crossing.units_sent, 40);
    assert_eq!(crossing.crossed, 40, "{crossing:?}");
    assert_eq!(crossing.waiting_near_side, 0, "{crossing:?}");
    assert!(crossing.all_arrived, "{crossing:?}");
    let (first, last) = (
        crossing.first_crossing_tick.expect("a first"),
        crossing.last_crossing_tick.expect("a last"),
    );
    assert!(0 < first && first < last, "{crossing:?}");
    assert!(
        (1..=40).contains(&crossing.crossed_per_minute_peak),
        "{crossing:?}"
    );
    assert!(window.service.battle.is_none(), "one army is no battle");
    assert_eq!(window.service.fights.hits, 0);
}

#[test]
fn two_armies_sent_across_the_same_bridge_meet_on_it_and_fight() {
    const ARMIES: &[Army] = &[
        Army {
            seat: 0,
            from: Side::Near,
            mix: &[(UnitKind::Militia, 15), (UnitKind::Archer, 10)],
        },
        Army {
            seat: 1,
            from: Side::Far,
            mix: &[(UnitKind::Militia, 15), (UnitKind::Archer, 10)],
        },
    ];
    let mut world = bridge_world();
    let choke: &'static Choke = Box::leak(Box::new(bridge_choke(&world)));
    let staging = Staging {
        map: "test",
        choke,
        armies: ARMIES,
        attack: true,
    };
    assert_eq!((staging.seats(), staging.units()), (2, 50));
    stage(&mut world, &staging, 1).expect("staged");
    let window = played(&mut world, &staging, 2_500);
    let battle = window.service.battle.expect("two armies make a battle");
    let first = battle.first_blow_tick.expect("they meet");
    assert!(first > 0, "{battle:?}");
    assert!(battle.fought > 0, "{battle:?}");
    let crossing = window.service.choke.expect("a choke");
    assert_eq!(
        battle.fought.min(crossing.units_sent) + battle.queued,
        crossing.units_sent,
        "every unit fought or only queued: {battle:?} {crossing:?}"
    );
    assert!(window.service.fights.hits > 0);
    assert!(
        window.service.fights.kills > 0,
        "{:?}",
        window.service.fights
    );
}

#[test]
fn a_staging_on_a_map_without_the_choke_is_refused() {
    let mut world = bridge_world();
    let elsewhere: &'static Choke = Box::leak(Box::new(Choke {
        saddle: (20, 12),
        ..bridge_choke(&world)
    }));
    let staging = Staging {
        map: "test",
        choke: elsewhere,
        armies: MILITIA,
        attack: false,
    };
    let error = stage(&mut world, &staging, 1).expect_err("no such neck here");
    assert!(error.contains("the first bridge"), "{error}");
    assert_eq!(world.unit_count(), 0, "nothing was placed");
}

#[test]
fn another_seed_stages_the_same_army_in_another_layout() {
    let staged_at = |seed| {
        let mut world = bridge_world();
        let choke: &'static Choke = Box::leak(Box::new(bridge_choke(&world)));
        let staging = Staging {
            map: "test",
            choke,
            armies: MILITIA,
            attack: false,
        };
        stage(&mut world, &staging, seed).expect("staged");
        world
            .units()
            .map(|unit| (unit.id.0, unit.position.x, unit.position.y))
            .collect::<Vec<_>>()
    };
    let (one, same, other) = (staged_at(1), staged_at(1), staged_at(2));
    assert_eq!(one, same, "a seed is a layout");
    assert_ne!(
        one, other,
        "another seed is another layout, though every unit is a Militia"
    );
    let places = |layout: &[(u32, i32, i32)]| -> BTreeSet<(i32, i32)> {
        layout.iter().map(|&(_, x, y)| (x, y)).collect()
    };
    assert_eq!(
        places(&one),
        places(&other),
        "the same places, dealt to the units in another order"
    );
}
