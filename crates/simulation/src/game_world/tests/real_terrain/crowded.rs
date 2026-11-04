//! Observe actual steering, rather than accepting a shared route as proof
//! that every body follows it. Diagnostics do not spend runtime work counters.
use super::landscapes::offered;
use super::*;
use crate::game_world::terrain_fixtures::{Command, build};
use metrics::Tracker;
use std::collections::BTreeSet;

#[derive(Debug, Default)]
struct Targets {
    blocked: BTreeSet<EntityId>,
    straight_lining: BTreeSet<EntityId>,
    longest_bad_ticks: u32,
}

fn clear(w: &GameWorld, kind: UnitKind, from: WorldPosition, to: WorldPosition) -> bool {
    let dx = i64::from(to.x) - i64::from(from.x);
    let dy = i64::from(to.y) - i64::from(from.y);
    let steps = ((dx.abs().max(dy.abs()) + 255) / 256).max(1);
    let mut previous = from;
    for n in 1..=steps {
        let next = WorldPosition::new(
            (i64::from(from.x) + dx * n / steps) as i32,
            (i64::from(from.y) + dy * n / steps) as i32,
        );
        if !w.body_step_clear(kind, previous, next) {
            return false;
        }
        previous = next;
    }
    true
}

fn run(count: usize, general: bool) {
    run_case(count, general, false);
}

fn run_case(count: usize, general: bool, town: bool) {
    let mut f = build(if town {
        "town"
    } else if general {
        "general-forest"
    } else {
        "forest"
    });
    if !general {
        // The fixture's original forty stand at the crowded exit. Command a
        // new compact column past them, then around the forest's offset gap.
        if town {
            for id in &f.ids {
                f.world.kill_unit(f.world.lookup[id]);
                f.world.remove_unit(*id);
            }
        }
        f.ids.clear();
        f.commands.clear();
        for n in 0..count {
            let kind = match n % 10 {
                0..=3 => UnitKind::Militia,
                4..=6 => UnitKind::Archer,
                7..=8 => UnitKind::Scout,
                _ => UnitKind::BatteringRam,
            };
            f.ids.push(
                f.world
                    .spawn_kind(
                        PlayerId(0),
                        kind,
                        center(22 + (n % 10) as i32, 31 + (n / 10) as i32),
                    )
                    .unwrap(),
            );
        }
        f.commands.push(Command::Group(
            f.ids.clone(),
            center(135, if town { 31 } else { 48 }),
        ));
    }
    for n in 0..30 {
        f.world
            .spawn_kind(
                PlayerId(0),
                UnitKind::Villager,
                center(41 + n % 5, 38 + n / 5),
            )
            .unwrap();
    }
    if town {
        // A base frontage beside the selected block: the shared route must
        // bend around an end before entering the town streets.
        for y in (18..63).step_by(3) {
            f.world
                .insert_building(
                    PlayerId(0),
                    BuildingKind::Barracks,
                    TileRect::from_xywh(33, y, 3, 3),
                    true,
                )
                .unwrap();
        }
    }
    let mut tracker = Tracker::new(&f.world, &f.ids);
    let mut targets = Targets::default();
    let mut bad_ticks = vec![0; f.ids.len()];
    let mut moving_samples = 0_u64;
    let mut pending_samples = 0_u64;
    let mut max_density = 0;
    let mut density_peak = None;
    f.issue();
    for tick in 1..=12_000 {
        f.world.advance();
        tracker.attach(&offered(&f));
        let arrived = tracker.observe(&f.world, tick);
        if tick % 10 == 0 {
            let accounting = std::mem::take(&mut f.world.group_work);
            for &id in &f.ids {
                let u = f.world.unit(id).unwrap();
                if f.world.formation_destination_slot(id) != Some(u.position) {
                    pending_samples += 1;
                    moving_samples += u64::from(u.position != u.previous_position);
                }
                let density = f
                    .ids
                    .iter()
                    .filter(|&&other| {
                        let p = f.world.unit(other).unwrap().position;
                        i64::from(p.x - u.position.x).pow(2) + i64::from(p.y - u.position.y).pow(2)
                            <= 1024_i64.pow(2)
                    })
                    .count();
                if density > max_density {
                    max_density = density;
                    density_peak = Some((tick, id, u.position, f.world.group_density_probe(id)));
                }
            }
            for (n, &id) in f.ids.iter().enumerate() {
                let index = f.world.lookup[&id];
                let u = f.world.unit(id).unwrap();
                let Some(motion) = f.world.formation_motion(index) else {
                    continue;
                };
                if motion.done {
                    continue;
                }
                let blocked = !f.world.body_position_clear(u.kind, motion.target);
                let straight = motion.target == motion.goal
                    && !clear(&f.world, u.kind, u.position, motion.target);
                if blocked {
                    targets.blocked.insert(id);
                }
                if straight {
                    targets.straight_lining.insert(id);
                }
                bad_ticks[n] = if blocked || straight {
                    bad_ticks[n] + 10
                } else {
                    0
                };
                targets.longest_bad_ticks = targets.longest_bad_ticks.max(bad_ticks[n]);
            }
            f.world.group_work = accounting;
        }
        if arrived {
            break;
        }
    }
    let result = tracker.finish(0);
    eprintln!(
        "crowded count={count} general={general} town={town} moving={moving_samples}/{pending_samples} max_bodies_in_one_tile_radius={max_density} peak={density_peak:?} {targets:?} {result:?}"
    );
    assert_eq!(result.arrived, f.ids.len(), "{result:?}; {targets:?}");
    assert!(result.max_stuck_ticks <= 60, "{result:?}; {targets:?}");
    assert!(targets.longest_bad_ticks <= 60, "{targets:?}");
    assert_eq!(result.max_speed_excess, 0, "{result:?}");
    assert_eq!(result.lost_group_ticks, 0, "{result:?}");
    if !general && count >= 100 {
        // The input block has five bodies in this disk. A bend may compress
        // it, but must not funnel the entire wide selection into one lane.
        assert!(
            max_density <= 12,
            "density={max_density} at {density_peak:?}"
        );
    }
}

#[test]
fn small_group_recovers_from_a_crowded_start() {
    run(16, false);
}
#[test]
fn hundred_without_general_recover_from_a_crowded_start() {
    run(100, false);
}
#[test]
fn two_hundred_without_general_recover_from_a_crowded_start() {
    run(200, false);
}
#[test]
fn general_hundred_recover_from_a_crowded_start() {
    run(100, true);
}

#[test]
fn hundred_leave_crowded_base_through_bending_town_streets() {
    run_case(100, false, true);
}

#[test]
fn two_hundred_leave_crowded_base_through_bending_town_streets() {
    run_case(200, false, true);
}
