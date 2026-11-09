//! Whole replays of crowded ordinary traffic, pinned on the unchanged engine.
//! Each scene is one the certificate could get wrong: an earlier mover of the
//! same tick closes a step that was clear in the snapshot.
use super::pin_values as pinned;
use super::{DX, DY, assert_pin, centre, open_world, pave, shifted, spawn, world_with_speed};
use crate::GameWorld;
use crate::UnitKind::{Knight, Militia, Scout, Spearman, Villager};

fn quiet(_: &mut GameWorld, _: usize) {}

/// Two columns of different players walk into each other on the same rows.
fn head_on_columns() -> GameWorld {
    let mut world = open_world();
    for n in 0..40 {
        let (file, row) = (3 * (n % 10), 100 + 2 * (n / 10));
        // A sub-tile phase per pair: first contacts sample every separation.
        let east = shifted(centre(50 + file, row), (n * 29) % 173 - 86, 0);
        let west = shifted(centre(84 + file, row), 83 - (n * 41) % 167, 0);
        let walker = spawn(&mut world, 1, Militia, east);
        let rider = spawn(&mut world, 2, Scout, west);
        world.issue_move(walker, centre(110 + file, row)).unwrap();
        world.issue_move(rider, centre(24 + file, row)).unwrap();
    }
    world
}

/// Riders catch up with slower walkers of another player on the same rows.
fn overtaking() -> GameWorld {
    let mut world = open_world();
    for n in 0..30 {
        let (file, row) = (2 * (n % 10), 100 + 2 * (n / 10));
        let at = shifted(centre(70 + file, row), (n * 37) % 151 - 75, 0);
        let walker = spawn(&mut world, 1, Villager, at);
        world.issue_move(walker, centre(130 + file, row)).unwrap();
    }
    for n in 0..18 {
        let (file, row) = (3 * (n % 6), 100 + 2 * (n / 6));
        let kind = [Knight, Scout, Scout][n as usize % 3];
        let rider = spawn(&mut world, 2, kind, centre(52 + file, row));
        world.issue_move(rider, centre(150 + file, row)).unwrap();
    }
    world
}

/// Riders too fast for a proposal (more than 512 subunits a tick) take the
/// sequential path and cover 700 subunits before a walker's turn.
fn dashing() -> GameWorld {
    let mut world = world_with_speed(700, 1);
    for n in 0..30 {
        let (file, row) = (3 * (n % 10), 100 + 2 * (n / 10));
        let at = shifted(centre(100 + file, row), (n * 37) % 151 - 75, 0);
        let walker = spawn(&mut world, 1, Villager, at);
        world.issue_move(walker, centre(150 + file, row)).unwrap();
    }
    for n in 0..24 {
        let (turn, row) = (3 * (n / 2), 100 + 2 * (n % 3));
        // Half dash east from behind, half west head-on, one after another.
        let (from, to) = [(10 + turn, 240), (245 - turn, 12)][n as usize % 2];
        let rider = spawn(&mut world, 2, Scout, centre(from, row));
        world.issue_move(rider, centre(to, row)).unwrap();
    }
    world
}

/// A paved row (+25% speed) with riders, oncoming walkers, unpaved lanes
/// beside it and walkers crossing it.
fn road_traffic() -> GameWorld {
    let mut world = open_world();
    for x in 60..120 {
        pave(&mut world, x, 100);
    }
    for n in 0..10 {
        let east = shifted(centre(60 + 2 * n, 100), (n * 53) % 161 - 80, 0);
        let west = shifted(centre(90 + 2 * n, 100), 80 - (n * 47) % 157, 0);
        let rider = spawn(&mut world, 2, Scout, east);
        let walker = spawn(&mut world, 1, Villager, west);
        world.issue_move(rider, centre(100 + 2 * n, 100)).unwrap();
        world.issue_move(walker, centre(62 + 2 * n, 100)).unwrap();
    }
    for n in 0..12 {
        let (lane, x) = (99 + 2 * (n % 2), 70 + 3 * n);
        let beside = spawn(&mut world, 1, Militia, centre(96 + 2 * n, lane));
        world.issue_move(beside, centre(64 + 2 * n, lane)).unwrap();
        let (from, to) = [(95, 105), (105, 95)][n as usize % 2];
        let crossing = spawn(&mut world, 3, Spearman, centre(x, from));
        world.issue_move(crossing, centre(x, to)).unwrap();
    }
    world
}

/// A standing body on every lane makes both streams sidestep while the other
/// stream passes them.
fn pickets() -> GameWorld {
    let mut world = open_world();
    for n in 0..12 {
        let row = 96 + n;
        spawn(&mut world, 3, Villager, centre(100 + 4 * (n % 2), row));
        for file in [0, 4] {
            let walker = spawn(&mut world, 1, Militia, centre(84 - file, row));
            let rider = spawn(&mut world, 2, Scout, centre(120 + file, row));
            world.issue_move(walker, centre(126 - file, row)).unwrap();
            world.issue_move(rider, centre(78 + file, row)).unwrap();
        }
    }
    world
}

/// Units sent to the same points from every side; two points hold a squatter.
/// Task moves keep the exact point, so several orders reserve one slot.
fn arrivals() -> GameWorld {
    let goals = [(90, 90), (110, 90), (90, 110), (110, 110), (100, 100)];
    let mut world = open_world();
    spawn(&mut world, 3, Villager, centre(90, 90));
    spawn(&mut world, 3, Villager, centre(110, 110));
    for n in 0..40_usize {
        let ((x, y), way) = (goals[n % 5], (n / 5) % 8);
        let far = 6 + (n % 4) as i32;
        let from = centre(x + far * DX[way], y + far * DY[way]);
        let kind = [Militia, Villager, Scout, Spearman][n % 4];
        let id = spawn(&mut world, 1 + (n % 2) as u16, kind, from);
        if n % 3 == 0 {
            world.issue_move(id, centre(x, y)).unwrap();
        } else {
            world.issue_task_move(id, centre(x, y)).unwrap();
        }
    }
    world
}

/// More than 32 bodies in one tile: sweeps sample the cell and outsiders must
/// route round until it drains. A second stack appears at tick 80.
fn overfull() -> GameWorld {
    let mut world = open_world();
    stack(&mut world, 100);
    for n in 0..9 {
        let (x, y, lane) = (88 + n / 3, 99 + n % 3, 98 + 4 * (n % 2));
        let rider = spawn(&mut world, 2, Scout, centre(x, y));
        world.issue_move(rider, centre(x + 26, y)).unwrap();
        let beside = spawn(&mut world, 3, Militia, centre(lane, 90 + n));
        world.issue_move(beside, centre(lane, 111 + n)).unwrap();
    }
    world
}

/// Forty villagers on one tile; every second one walks out to a ring.
fn stack(world: &mut GameWorld, x: i32) {
    for n in 0..40 {
        let at = shifted(centre(x, 100), 9 * (n % 8) - 32, 11 * (n / 8) - 22);
        let id = spawn(world, 1, Villager, at);
        if n % 2 == 0 {
            let way = (n / 2 % 8) as usize;
            let ring = centre(x + 8 * DX[way], 100 + 8 * DY[way]);
            world.issue_move(id, ring).unwrap();
        }
    }
}

fn restack(world: &mut GameWorld, tick: usize) {
    if tick == 80 {
        stack(world, 106);
    }
}

#[test]
fn two_columns_crossing_head_on_replay_unchanged() {
    assert_pin("head_on", head_on_columns, 400, quiet, &pinned::HEAD_ON);
}

#[test]
fn fast_units_overtaking_slow_ones_replay_unchanged() {
    assert_pin("overtaking", overtaking, 400, quiet, &pinned::OVERTAKING);
}

#[test]
fn riders_faster_than_a_proposal_replay_unchanged() {
    assert_pin("dashing", dashing, 300, quiet, &pinned::DASHING);
}

#[test]
fn movers_on_a_road_replay_unchanged() {
    assert_pin("road", road_traffic, 350, quiet, &pinned::ROAD);
}

#[test]
fn movers_passing_sidestepping_units_replay_unchanged() {
    assert_pin("pickets", pickets, 400, quiet, &pinned::PICKETS);
}

#[test]
fn units_arriving_at_occupied_slots_replay_unchanged() {
    assert_pin("arrivals", arrivals, 400, quiet, &pinned::ARRIVALS);
}

#[test]
fn movers_beside_overfull_cells_replay_unchanged() {
    assert_pin("overfull", overfull, 300, restack, &pinned::OVERFULL);
}
