//! The PR 4 showcase, deterministically: 24 militia in a 6x4 block attack-move
//! through five militia standing their ground. AoE2 is the standard: only a
//! unit that itself sees an enemy breaks off; the rest keep the formation's
//! pace; a melee unit walks until in reach and swings only while in reach.
use super::*;

/// Every stationary attacker must be in reach, including route waiters.
fn attack_pose(u: &crate::GameUnit) -> bool {
    !u.moving && u.task == UnitTask::Attacking
}

fn subunits(a: WorldPosition, b: WorldPosition) -> i64 {
    let (x, y) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    (x * x + y * y).isqrt()
}

/// What the probe saw; distances are hundredths of a tile.
#[derive(Debug, Default)]
struct Front {
    /// The tick the first member broke off to fight.
    first_engage: Option<u32>,
    /// `(engaged so far, members still in the group, of those moved that
    /// tick, units in the attack pose out of reach)`, 2 s and 5 s after the
    /// first member broke off.
    at_2s: (usize, usize, usize, usize),
    at_5s: (usize, usize, usize, usize),
    /// Member-ticks in the group that did not move, from the first contact
    /// until the enemies died.
    halted_member_ticks: u32,
    /// Thousandths of a tile moved per member-tick in the group while
    /// fighting, against the militia's own speed.
    group_pace: i64,
    speed: i64,
    /// Farthest distance to its target at the tick a member broke off.
    max_engage_distance: i64,
    /// Farthest distance to its target of a unit in the attack pose, and
    /// member-ticks in that pose out of reach.
    max_pose_distance: i64,
    pose_out_of_reach_ticks: u32,
    /// Farthest attacker distance to its target at a damaging blow.
    max_hit_distance: i64,
    hits: u32,
    /// Distinct members that ever broke off to fight.
    engaged_total: usize,
    cleared: Option<u32>,
    /// Approaches deferred because every melee spot was taken, or because
    /// the tick's position-search budget was spent.
    waits: (u64, u64),
}

fn showcase() -> (GameWorld, Vec<EntityId>, Vec<EntityId>) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    // Six columns and four rows, 1.3 tiles apart, centred on (101.5, 29.5).
    let ids = (0..24)
        .map(|k| {
            let (c, r) = (k % 6, k / 6);
            let at = WorldPosition::new(
                101 * 1024 + 512 + (c * 2 - 5) * 666,
                29 * 1024 + 512 + (r * 2 - 3) * 666,
            );
            w.spawn_kind(PlayerId(0), UnitKind::Militia, at).unwrap()
        })
        .collect::<Vec<_>>();
    let foes = [(101, 19), (102, 18), (100, 20), (102, 20), (100, 18)]
        .into_iter()
        .map(|(x, y)| {
            w.spawn_kind(PlayerId(1), UnitKind::Militia, center(x, y))
                .unwrap()
        })
        .collect::<Vec<_>>();
    w.issue_stance(PlayerId(1), &foes, CombatStance::StandGround)
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(101, 8),
        false,
        true,
    )
    .unwrap();
    (w, ids, foes)
}

fn run(w: &mut GameWorld, ids: &[EntityId], foes: &[EntityId], ticks: u32) -> Front {
    let mut out = Front::default();
    let mut engaged = std::collections::BTreeSet::new();
    let mut hp = foes
        .iter()
        .map(|&f| w.unit(f).unwrap().hp)
        .collect::<Vec<_>>();
    let range = super::super::super::navy::attack_range(&w.unit_stats(ids[0]).unwrap());
    let (speed, denom) = w.movement_speed(UnitKind::Militia, PlayerId(0));
    out.speed = (speed * 1000 / 1024 / denom) as i64;
    let (mut moved, mut member_ticks) = (0, 0);
    let to_target = |w: &GameWorld, u: &crate::GameUnit| {
        let target = w.unit(u.attack_target?)?;
        Some(subunits(u.position, target.position))
    };
    for tick in 1..=ticks {
        let before = ids
            .iter()
            .map(|&id| w.unit(id).map(|u| u.position))
            .collect::<Vec<_>>();
        // Movement/spacing precede combat. In this flat scene positions stay
        // fixed afterwards: measure the actual victim and attacker at the blow,
        // rather than the old target and positions before this tick's steps.
        w.advance();
        for (n, &f) in foes.iter().enumerate() {
            let Some(u) = w.unit(f) else { continue };
            if u.hp < hp[n] {
                out.hits += 1;
                let striker = u.last_hit.unwrap().attacker;
                if ids.contains(&striker)
                    && let Some(attacker) = w.unit(striker)
                {
                    let d = subunits(attacker.position, u.position) * 100 / 1024;
                    out.max_hit_distance = out.max_hit_distance.max(d);
                }
            }
            hp[n] = u.hp;
        }
        let fighting = out.cleared.is_none();
        let (mut in_group, mut marching, mut far_poses) = (0, 0, 0);
        for (n, &id) in ids.iter().enumerate() {
            let Some(u) = w.unit(id).filter(|u| u.hp > 0) else {
                continue;
            };
            let distance = to_target(w, &u);
            if u.attack_target.is_some() && engaged.insert(id) {
                out.first_engage.get_or_insert(tick);
                let d = distance.unwrap_or(0) * 100 / 1024;
                out.max_engage_distance = out.max_engage_distance.max(d);
            }
            if w.formation(id).is_some() {
                in_group += 1;
                let step = before[n].map_or(0, |b| subunits(b, u.position));
                marching += usize::from(step > 0);
                if out.first_engage.is_some() && fighting {
                    out.halted_member_ticks += u32::from(step == 0);
                    moved += step;
                    member_ticks += 1;
                }
            }
            if attack_pose(&u)
                && let Some(d) = distance
            {
                out.max_pose_distance = out.max_pose_distance.max(d * 100 / 1024);
                if d > range {
                    out.pose_out_of_reach_ticks += 1;
                    far_poses += 1;
                }
            }
        }
        let sample = (engaged.len(), in_group, marching, far_poses);
        match out.first_engage.map(|first| tick - first) {
            Some(40) => out.at_2s = sample,
            Some(100) => out.at_5s = sample,
            _ => {}
        }
        if out.cleared.is_none() && foes.iter().all(|&f| w.unit(f).is_none_or(|u| u.hp == 0)) {
            out.cleared = Some(tick);
        }
    }
    out.engaged_total = engaged.len();
    out.group_pace = moved * 1000 / 1024 / member_ticks.max(1);
    out.waits = w.combat.waits;
    out
}

#[test]
fn attack_move_front_engages_alone_while_the_rest_march_on() {
    let (mut w, ids, foes) = showcase();
    let front = run(&mut w, &ids, &foes, 1200);
    eprintln!("group-baseline attack-move-front: {front:?}");
    // Before: a member broke off for any enemy in a 13x13-tile window around
    // it (up to 8.40 tiles away; militia sight is 4) from tick 37, and with
    // every melee spot around the five taken it stood still in the attack
    // pose: 5 s after the first, 19 of 24 had broken off and 13 stood out of
    // reach, up to 8.40 tiles from their target, 6,871 member-ticks in all
    // (373 crowded waits). The group itself kept 94 % of militia speed.
    // Blows only ever landed within reach (0.96 tiles at most).
    assert_eq!(front.first_engage, Some(81));
    assert!(front.max_engage_distance <= 400, "{front:?}");
    // PR 10 legal contact engages 19 members by five seconds.
    assert_eq!((front.at_2s, front.at_5s), ((10, 14, 14, 0), (19, 5, 5, 0)));
    // PR 10 approach strikes use the full one-tile reach without false poses.
    assert_eq!(
        (front.pose_out_of_reach_ticks, front.max_pose_distance),
        (0, 100)
    );
    assert!(front.max_hit_distance <= 100, "{front:?}");
    assert!(front.group_pace * 100 >= front.speed * 90, "{front:?}");
    // PR 10 reachable contact clears the front at tick 341.
    assert_eq!((front.cleared, front.engaged_total), (Some(341), 24));
    // Every one of the 24 survives and stands at its slot.
    assert!(ids.iter().all(|&id| {
        w.unit(id).is_some_and(|u| u.hp > 0 && !u.moving)
            && tiles_between(w.unit(id).unwrap().position, center(101, 8)) <= 6
    }));
}

#[test]
fn a_stop_cancels_an_attack_move_members_way_back_to_its_slot() {
    let (mut w, ids, _) = showcase();
    let mut waiting = None;
    for _ in 0..1200 {
        w.advance();
        waiting = ids
            .iter()
            .copied()
            .find(|id| w.combat.resuming.contains_key(id));
        if waiting.is_some() {
            break;
        }
    }
    let id = waiting.expect("a member waits to march on after its fight");
    w.issue_stop(PlayerId(0), id).unwrap();
    assert!(!w.combat.resuming.contains_key(&id));
    assert_eq!(w.formation(id), None);
    assert!(w.movement_order(id).is_none());
    let at = w.unit(id).unwrap().position;
    // The faster fight can leave other combatants nearby at the first resume.
    // Isolate cancellation from fresh acquisition and friendly idle pushes.
    w.issue_stance(PlayerId(0), &[id], CombatStance::NoAttack)
        .unwrap();
    let others = w
        .units()
        .filter(|u| u.id != id)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    for other in others {
        w.remove_unit(other);
    }
    for _ in 0..200 {
        w.advance();
    }
    // The cancelled member has no way back to its slot.
    let u = w.unit(id).unwrap();
    assert_eq!(w.formation(id), None);
    assert!(!u.moving && u.position == at, "{u:?}");
}
