//! Units meeting head-on must commit to one side and get past each other.
use super::*;
use crate::UnitKind;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Counts changes of the sign of the lateral offset, ignoring the dead band
/// around the exact centre line where a body is not yet on either side.
fn side_flips(trace: &[i32], line: i32) -> usize {
    let mut last = 0;
    let mut flips = 0;
    for &y in trace {
        let side = match y - line {
            d if d > 96 => 1,
            d if d < -96 => -1,
            _ => continue,
        };
        if last != 0 && side != last {
            flips += 1;
        }
        last = side;
    }
    flips
}

fn meet(a_kind: UnitKind, b_kind: UnitKind, same_player: bool, corridor: bool, standing: bool) {
    meet_offset(a_kind, b_kind, same_player, corridor, standing, 0);
}

fn meet_offset(
    a_kind: UnitKind,
    b_kind: UnitKind,
    same_player: bool,
    corridor: bool,
    standing: bool,
    offset: i32,
) {
    let mut w = world();
    w.set_fog_disabled(true);
    if corridor {
        for x in 34..47 {
            for y in [39, 41] {
                w.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
    }
    let (left, right) = (at(30, 40), at(50, 40));
    let shifted = |p: WorldPosition| WorldPosition::new(p.x, p.y + offset);
    let a = w.spawn_kind(PlayerId(0), a_kind, left).unwrap();
    let b_start = if standing { at(40, 40) } else { right };
    let b_player = PlayerId(u16::from(!same_player));
    let b = w.spawn_kind(b_player, b_kind, shifted(b_start)).unwrap();
    w.issue_move(a, right).unwrap();
    if !standing {
        w.issue_move(b, shifted(left)).unwrap();
    }
    let line = 40 * 1024 + 512;
    let (mut ta, mut tb) = (Vec::new(), Vec::new());
    let mut done = None;
    for tick in 0..900 {
        w.advance();
        ta.push(w.unit(a).unwrap().position.y);
        tb.push(w.unit(b).unwrap().position.y);
        let (pa, pb) = (w.unit(a).unwrap().position, w.unit(b).unwrap().position);
        let a_there = (pa.x - right.x).abs() < 1024 * 2;
        let b_there = standing || (pb.x - left.x).abs() < 1024 * 2;
        if a_there && b_there {
            done = Some(tick);
            break;
        }
    }
    let label = format!(
        "{a_kind:?} vs {b_kind:?} same={same_player} off={offset} corridor={corridor} standing={standing}"
    );
    let tick = done.unwrap_or_else(|| {
        panic!(
            "{label}: no pass; a={:?} b={:?} flips={} {}",
            w.unit(a).unwrap().position,
            w.unit(b).unwrap().position,
            side_flips(&ta, line),
            side_flips(&tb, line)
        )
    });
    assert!(tick < 600, "{label}: passed only at tick {tick}");
    assert!(
        side_flips(&ta, line) <= 1,
        "{label}: a flipped {}",
        side_flips(&ta, line)
    );
    assert!(
        side_flips(&tb, line) <= 1,
        "{label}: b flipped {}",
        side_flips(&tb, line)
    );
}

#[test]
fn villagers_pass_each_other_in_the_open() {
    meet(UnitKind::Villager, UnitKind::Villager, true, false, false);
}

#[test]
fn villagers_of_different_players_pass_in_the_open() {
    meet(UnitKind::Villager, UnitKind::Villager, false, false, false);
}

#[test]
fn villager_and_soldier_pass_in_the_open() {
    meet(UnitKind::Villager, UnitKind::Militia, true, false, false);
}

#[test]
fn villagers_pass_each_other_in_a_one_tile_corridor() {
    meet(UnitKind::Villager, UnitKind::Villager, true, true, false);
}

#[test]
fn villager_and_soldier_pass_in_a_one_tile_corridor() {
    meet(UnitKind::Villager, UnitKind::Militia, true, true, false);
}

#[test]
fn a_walker_passes_a_standing_unit_dead_centre() {
    meet(UnitKind::Villager, UnitKind::Villager, true, false, true);
}

#[test]
fn a_walker_passes_a_standing_soldier_in_a_corridor() {
    meet(UnitKind::Villager, UnitKind::Militia, true, true, true);
}

#[test]
fn lateral_offsets_never_make_a_pair_alternate_sides() {
    for kinds in [
        (UnitKind::Villager, UnitKind::Villager),
        (UnitKind::Villager, UnitKind::Militia),
    ] {
        for standing in [false, true] {
            for corridor in [false, true] {
                for offset in [-400, -150, -30, 0, 30, 150, 400] {
                    meet_offset(kinds.0, kinds.1, true, corridor, standing, offset);
                }
            }
        }
    }
}

#[test]
fn equal_cost_detours_go_to_the_right_of_each_heading() {
    // A standing body dead ahead: the detour round it costs the same either
    // way. Units walking in opposite directions must each pick their own right,
    // so that they pass each other instead of meeting on the same side again.
    let detour = |from: i32, to: i32| {
        let mut w = world();
        w.spawn_villager(PlayerId(0), at(40, 40)).unwrap();
        let id = w.spawn_villager(PlayerId(0), at(from, 40)).unwrap();
        let index = w.lookup[&id];
        w.local_refinement_left = 4;
        let path = w.local_unit_route(index, at(to, 40)).expect("a detour");
        path.iter().map(|p| p.y - 40 * 1024 - 512).sum::<i32>()
    };
    let east = detour(39, 41);
    let west = detour(41, 39);
    assert!(
        east > 0,
        "east-bound detour must keep right (south): {east}"
    );
    assert!(
        west < 0,
        "west-bound detour must keep right (north): {west}"
    );
}
