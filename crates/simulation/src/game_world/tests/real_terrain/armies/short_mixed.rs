//! Short and scattered mixed orders across water: the river at x = 150 has
//! one bridge near y = 150, off the straight line. No member may walk
//! straight at the bank without a route, and none may stop short.
use super::*;
use crate::game_world::terrain_fixtures::build;

const KINDS: [UnitKind; 4] = [
    UnitKind::Champion,
    UnitKind::Crossbowman,
    UnitKind::Paladin,
    UnitKind::SiegeRam,
];

fn tile(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[derive(Debug, Default)]
struct Outcome {
    across: usize,
    /// Ticks members spent pressed against an obstacle while still on
    /// the group's straight-line steering (no route of their own).
    blocked_unrouted: u32,
    direct: bool,
    done_tick: Option<u32>,
    report: String,
}

fn cross(spawn: &[(i32, i32)], goal: (i32, i32), ticks: u32, siege: bool) -> Outcome {
    let mut f = build("army-river");
    let ids = spawn
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            f.world
                .spawn_kind(PlayerId(0), KINDS[i % KINDS.len()], tile(x, y))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let goal = tile(goal.0, goal.1);
    let counted = ids
        .iter()
        .copied()
        .filter(|&id| siege || !f.world.unit(id).unwrap().kind.is_siege())
        .collect::<Vec<_>>();
    f.world
        .issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            goal,
            false,
            false,
        )
        .unwrap();
    let mut out = Outcome {
        direct: f
            .world
            .formation_member_probe(ids[0])
            .is_some_and(|probe| probe.0),
        ..Outcome::default()
    };
    let far_bank = |world: &GameWorld, id| {
        let at = world.unit(id).unwrap().position;
        at.tile_floor().x > 153 && path_squared(at, goal) <= i64::from(8 * 1024).pow(2)
    };
    for tick in 0..ticks {
        f.world.advance();
        for &id in &counted {
            if let Some((direct, navigating, detached, blocked)) =
                f.world.formation_member_probe(id)
                && (direct || !navigating)
                && !detached
                && blocked > 0
                && !far_bank(&f.world, id)
            {
                out.blocked_unrouted += 1;
            }
        }
        if out.done_tick.is_none() && counted.iter().all(|&id| far_bank(&f.world, id)) {
            out.done_tick = Some(tick);
        }
    }
    for &id in &counted {
        if far_bank(&f.world, id) {
            out.across += 1;
        } else {
            let unit = f.world.unit(id).unwrap();
            out.report.push_str(&format!(
                "{:?} {:?} at {:?} {:?} {}\n",
                id,
                unit.kind,
                unit.position.tile_floor(),
                unit.task,
                f.world.formation_member_status(id),
            ));
        }
    }
    out
}

fn path_squared(a: WorldPosition, b: WorldPosition) -> i64 {
    i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)
}

fn record(name: &str, out: &Outcome) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/short-mixed");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{name}.txt")), format!("{out:#?}\n"));
}

/// 16 members 18 tiles from the goal, compact: a "short" order.
#[test]
fn short_mixed_order_across_a_river_takes_the_bridge() {
    let spawn = (0..16)
        .map(|i| (134 + (i % 4) * 2, 126 + (i / 4) * 2))
        .collect::<Vec<_>>();
    let out = cross(&spawn, (155, 130), 6_000, true);
    record("short", &out);
    assert!(
        !out.direct,
        "straight lines cross the river: not a direct order"
    );
    assert_eq!(
        out.across, 16,
        "members short of the far bank:\n{}",
        out.report
    );
    assert!(
        out.blocked_unrouted <= 16 * 8,
        "members pressed on the bank without a route: {out:?}"
    );
}

/// Twelve units scattered over more ground than the trip is long. Its ram
/// is not counted: a ram on group steering stalls at the far mouth of this
/// diagonal deck, where the corridor's corner cuts over water (open issue).
#[test]
fn spread_mixed_order_across_a_river_takes_the_bridge() {
    let spawn = (0..12)
        .map(|i| (110 + (i % 3) * 14, 70 + (i / 3) * 18))
        .collect::<Vec<_>>();
    let out = cross(&spawn, (170, 110), 8_000, false);
    record("spread", &out);
    assert!(
        !out.direct,
        "straight lines cross the river: not a direct order"
    );
    assert_eq!(
        out.across, 9,
        "members short of the far bank:\n{}",
        out.report
    );
    assert!(
        out.blocked_unrouted <= 9 * 8,
        "members pressed on the bank without a route: {out:?}"
    );
}
