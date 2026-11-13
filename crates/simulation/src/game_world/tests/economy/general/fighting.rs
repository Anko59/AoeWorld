//! Marching armies fight: enemies in sight split the army into small groups
//! that clear them with attack-move behaviour, then regroup around the General.
use super::*;

fn muster(w: &mut GameWorld, count: i32, x: i32, y: i32) -> Vec<EntityId> {
    (0..count)
        .map(|n| soldier(w, UnitKind::Militia, x + n % 8, y + n / 8))
        .collect()
}

fn enemies(w: &mut GameWorld, count: i32, x: i32, y: i32, spacing: i32) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            let id = w
                .spawn_kind(
                    PlayerId(1),
                    UnitKind::Militia,
                    at(x + n % 4 * spacing, y + n / 4 * spacing),
                )
                .unwrap();
            w.units[w.lookup[&id]].state.stance = aoe_core::CombatStance::StandGround;
            id
        })
        .collect()
}

fn alive(w: &GameWorld, ids: &[EntityId]) -> usize {
    ids.iter()
        .filter(|&&id| w.unit(id).is_some_and(|u| u.hp > 0))
        .count()
}

#[test]
fn soldier_sight_beyond_the_adoption_radius_triggers_fighting() {
    for offset in [25, 30, 50] {
        let (mut w, _) = setup();
        let army = muster(&mut w, 12, 100, 100);
        let g = soldier(&mut w, UnitKind::General, 104, 104);
        let mut u = w.unit(army[0]).unwrap();
        w.remove_unit(army[0]);
        u.position = at(104 + offset, 104);
        u.previous_position = u.position;
        w.insert_map_unit(u, 0);
        let enemy = w
            .spawn_kind(PlayerId(1), UnitKind::Militia, at(105 + offset, 104))
            .unwrap();
        assert!(w.unit_visible_to(PlayerId(0), enemy));
        decide(&mut w);
        assert_eq!(
            w.general_status(g).unwrap().state,
            GeneralState::Fighting,
            "soldier at {offset}"
        );
        assert!(w.general_work().orders > 0);
        assert!(w.general_work().unit_visits <= 1024);
        // Enlarging enemy discovery must not enlarge adoption.
        let outsider = soldier(&mut w, UnitKind::Militia, 139, 110);
        decide(&mut w);
        assert_eq!(w.general_controller(outsider), None);
    }
}

fn tiles(a: WorldPosition, b: WorldPosition) -> i64 {
    (i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)).isqrt() / 1024
}

#[test]
fn an_enemy_camp_is_cleared_instead_of_idled_in() {
    let (mut w, _) = setup();
    let army = muster(&mut w, 40, 100, 100);
    let g = soldier(&mut w, UnitKind::General, 104, 104);
    let camp = enemies(&mut w, 16, 140, 90, 6);
    // Keep both halves visible: otherwise the army can finish its march with
    // untouched enemies hidden in fog. Outposts supply sight, never damage.
    for y in [94, 106] {
        w.insert_building(
            PlayerId(0),
            BuildingKind::Outpost,
            TileRect::from_xywh(149, y, 1, 1),
            true,
        )
        .unwrap();
    }
    assert!(camp.iter().all(|&id| w.unit_visible_to(PlayerId(0), id)));
    w.issue_move(g, at(148, 98)).unwrap();
    // Longest run of ticks an army member stood idle with a camp member
    // within its own sight: ordinary attack-move does not use the commander's
    // or the outposts' sight to pick targets.
    let mut idle = vec![0_u32; army.len()];
    let mut worst = 0;
    let mut cleared = None;
    for tick in 0..6000 {
        w.advance();
        if cleared.is_none() && alive(&w, &camp) == 0 {
            cleared = Some(tick);
        }
        if tick < 400 {
            continue;
        }
        for (n, &id) in army.iter().enumerate() {
            let Some(u) = w.unit(id).filter(|u| u.hp > 0) else {
                continue;
            };
            let sight = i64::from(w.unit_observer(&u).unwrap().2) * 1024;
            let near = camp.iter().filter_map(|&e| w.unit(e)).any(|e| {
                e.hp > 0
                    && i64::from(e.position.x - u.position.x).pow(2)
                        + i64::from(e.position.y - u.position.y).pow(2)
                        <= sight * sight
            });
            idle[n] = if near && !u.moving && !u.planning && u.attack_target.is_none() {
                idle[n] + 1
            } else {
                0
            };
            if idle[n] == 101 {
                let nearest = camp
                    .iter()
                    .filter_map(|&e| w.unit(e))
                    .filter(|e| e.hp > 0)
                    .map(|e| (tiles(e.position, u.position), e.id))
                    .min();
                eprintln!(
                    "camp idle: id={id:?} at={:?} los={:?} nearest={nearest:?} stance={:?} task={:?} formation={:?}",
                    u.position,
                    w.unit_observer(&u).map(|(_, _, los)| los),
                    u.stance,
                    u.task,
                    w.formation(id)
                );
            }
            worst = worst.max(idle[n]);
        }
    }
    assert_eq!(alive(&w, &camp), 0, "the camp is cleared");
    assert!(alive(&w, &army) >= 30);
    assert!(
        worst <= 100,
        "a soldier idled {worst} ticks beside the camp"
    );
}

#[test]
fn engaging_an_enemy_splits_the_army_into_groups_of_at_most_twenty() {
    let (mut w, _) = setup();
    let army = muster(&mut w, 60, 100, 100);
    let g = soldier(&mut w, UnitKind::General, 104, 108);
    enemies(&mut w, 4, 124, 104, 3);
    w.issue_move(g, at(160, 106)).unwrap();
    let mut most = 0;
    let mut groups_seen = 0;
    for _ in 0..600 {
        w.advance();
        let mut sizes = std::collections::BTreeMap::new();
        for &id in &army {
            if w.general_status(g).unwrap().state == GeneralState::Fighting
                && let Some(leader) = w.units[w.lookup[&id]].formation
            {
                *sizes.entry(leader).or_insert(0_usize) += 1;
            }
        }
        most = most.max(sizes.values().copied().max().unwrap_or(0));
        groups_seen = groups_seen.max(sizes.len());
    }
    assert!(
        groups_seen >= 3,
        "60 soldiers fight as {groups_seen} groups"
    );
    assert!(most <= 20, "largest fighting group has {most}");
}

#[test]
fn the_army_regroups_around_its_general_when_no_enemy_remains() {
    let (mut w, _) = setup();
    let army = muster(&mut w, 30, 100, 100);
    let g = soldier(&mut w, UnitKind::General, 104, 104);
    // Pickets stand ground ten tiles out: the army must walk to them and, with
    // no march order, would otherwise be left wherever the fight ended.
    let pickets = enemies(&mut w, 4, 112, 100, 2);
    advance(&mut w, 2500);
    assert_eq!(alive(&w, &pickets), 0);
    let p = w.unit(g).unwrap().position;
    let spread = army
        .iter()
        .filter_map(|&id| w.unit(id))
        .filter(|u| u.hp > 0)
        .map(|u| tiles(u.position, p))
        .max()
        .unwrap();
    assert!(
        spread <= 12,
        "survivors are {spread} tiles from the General"
    );
    assert!(
        army.iter().filter_map(|&id| w.unit(id)).all(|u| !u.moving),
        "the regroup has settled"
    );
}
