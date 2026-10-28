//! Tile-jam scenarios from the 2026-10-28 playtest ("too strict exclusivity"):
//! units that meet in a one-tile corridor, and two crowds that cross. Each
//! reports ticks until everyone arrived and the longest time any unit stood
//! still. Ticks are 20 Hz.
use super::*;

/// Walls above and below row `y`, `x0..x1` long: a corridor one tile wide.
fn corridor(w: &mut GameWorld, y: i32, x0: i32, x1: i32) {
    forest(w, TileRect::from_xywh(x0, y - 1, x1 - x0, 1));
    forest(w, TileRect::from_xywh(x0, y + 1, x1 - x0, 1));
}

fn arrived(w: &GameWorld, id: EntityId, goal: WorldPosition) -> bool {
    let u = w.unit(id).unwrap();
    !u.moving && tiles_between(u.position, goal) < 2
}

/// `(tick everyone arrived, never arrived, longest stand-still of any unit)`.
pub(super) fn swap(w: &mut GameWorld, pairs: &[(EntityId, WorldPosition)]) -> (u32, usize, u32) {
    let mut previous = pairs
        .iter()
        .map(|&(id, _)| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    let mut still = vec![0_u32; pairs.len()];
    let (mut worst, mut done_at) = (0, 0);
    for tick in 1..=1600_u32 {
        w.advance();
        for (n, &(id, goal)) in pairs.iter().enumerate() {
            let at = w.unit(id).unwrap().position;
            if at == previous[n] && !arrived(w, id, goal) {
                still[n] += 1;
                worst = worst.max(still[n]);
            } else {
                still[n] = 0;
            }
            previous[n] = at;
        }
        if done_at == 0 && pairs.iter().all(|&(id, g)| arrived(w, id, g)) {
            done_at = tick;
        }
    }
    let never = pairs.iter().filter(|&&(id, g)| !arrived(w, id, g)).count();
    (done_at, never, worst)
}

/// `west`/`east` villagers of players `a`/`b` swap ends of a corridor.
fn corridor_swap(a: u16, b: u16, each: i32) -> (u32, usize, u32) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    corridor(&mut w, 40, 30, 70);
    let mut pairs = Vec::new();
    for n in 0..each {
        let (west, east) = (center(33 + n, 40), center(66 - n, 40));
        let one = w.spawn_villager(PlayerId(a), west).unwrap();
        let two = w.spawn_villager(PlayerId(b), east).unwrap();
        w.issue_move(one, center(68 - n, 40)).unwrap();
        w.issue_move(two, center(31 + n, 40)).unwrap();
        pairs.push((one, w.movement_order(one).unwrap().destination));
        pairs.push((two, w.movement_order(two).unwrap().destination));
    }
    swap(&mut w, &pairs)
}

#[test]
fn villagers_meeting_head_on_in_a_one_tile_corridor() {
    // Before PR 7 (hard bodies and the 64-tick friendly squeeze): 1, 2 and 3
    // villagers each way arrived at ticks 880, 907 and 949 for friends and
    // enemies alike, with the same 22-tick stand-still for 2-3 each way. The
    // corridor was never deadlocked; it stays that way (arrivals at 878-908).
    for (a, b) in [(0, 0), (0, 1)] {
        for each in 1..=3 {
            let (done, never, still) = corridor_swap(a, b, each);
            assert_eq!(never, 0, "players {a}/{b}, {each} each");
            assert!((1..=1000).contains(&done), "arrived at {done}");
            assert!(
                still <= 30,
                "stood still {still} ticks: players {a}/{b}, {each} each"
            );
        }
    }
}

/// Two groups of 40 walk through each other on open ground. Returns the arrival
/// tick, units that never arrived, the longest stand-still, and the closest any
/// pair came as a percentage of its radii sum.
fn crossing(a: u16, b: u16) -> (u32, usize, u32, i64) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let mut pairs = Vec::new();
    for n in 0..40 {
        let (col, row) = (n % 5, n / 5);
        let one = w
            .spawn_villager(PlayerId(a), center(40 + col, 60 + row))
            .unwrap();
        let two = w
            .spawn_villager(PlayerId(b), center(60 - col, 60 + row))
            .unwrap();
        w.issue_move(one, center(60 - col, 60 + row)).unwrap();
        w.issue_move(two, center(40 + col, 60 + row)).unwrap();
        pairs.push((one, w.movement_order(one).unwrap().destination));
        pairs.push((two, w.movement_order(two).unwrap().destination));
    }
    let ids = pairs.iter().map(|p| p.0).collect::<Vec<_>>();
    let mut closest = i64::MAX;
    let mut probe = |w: &GameWorld| {
        for (n, &x) in ids.iter().enumerate() {
            for &y in &ids[n + 1..] {
                let (p, q) = (w.unit(x).unwrap(), w.unit(y).unwrap());
                let sum = i64::from(p.kind.collision_radius() + q.kind.collision_radius());
                let d = (i64::from(p.position.x) - i64::from(q.position.x)).pow(2)
                    + (i64::from(p.position.y) - i64::from(q.position.y)).pow(2);
                closest = closest.min(d.isqrt() * 100 / sum);
            }
        }
    };
    let (mut done, mut previous) = (
        0,
        ids.iter()
            .map(|&i| w.unit(i).unwrap().position)
            .collect::<Vec<_>>(),
    );
    let (mut still, mut worst) = (vec![0_u32; ids.len()], 0);
    for tick in 1..=1200_u32 {
        w.advance();
        probe(&w);
        for (n, &(id, goal)) in pairs.iter().enumerate() {
            let at = w.unit(id).unwrap().position;
            if at == previous[n] && !arrived(&w, id, goal) {
                still[n] += 1;
                worst = worst.max(still[n]);
            } else {
                still[n] = 0;
            }
            previous[n] = at;
        }
        if done == 0 && pairs.iter().all(|&(id, g)| arrived(&w, id, g)) {
            done = tick;
        }
    }
    let never = pairs.iter().filter(|&&(id, g)| !arrived(&w, id, g)).count();
    (done, never, worst, closest)
}

#[test]
fn two_crossing_groups_of_forty_arrive_without_a_frozen_unit() {
    for (a, b) in [(0, 0), (0, 1)] {
        let (done, never, still, closest) = crossing(a, b);
        eprintln!(
            "group-baseline crossing {a}/{b}: done={done} never={never} still={still} closest={closest}%"
        );
        assert_eq!(never, 0, "players {a}/{b}");
        assert!(
            still <= 20,
            "a unit stood still {still} ticks: players {a}/{b}"
        );
        // No pair ever breaks its hard distance (design 4.3 step 2): half of
        // the radii sum for own moving traffic, the whole sum for other players.
        assert!(
            closest >= if a == b { 50 } else { 100 },
            "closest {closest}%"
        );
    }
}
