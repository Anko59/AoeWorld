use super::*;

#[test]
fn vassal_domains_are_spread_and_each_is_visited_every_period() {
    let (mut w, first, _) = setup();
    let mut ids = vec![first];
    for x in (104..=200).step_by(12) {
        ids.push(garrison_domain(&mut w, x));
    }
    advance(&mut w, 2);
    for &id in &ids {
        w.issue_vassal_development(PlayerId(0), id, false).unwrap();
    }
    let mut visited = Vec::new();
    for tick in 20..60 {
        w.tick.0 = tick;
        w.advance_vassals(&mut Vec::new());
        let next: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| w.vassal_status(*id).unwrap().wait == crate::VassalWait::Paused)
            .filter(|id| !visited.contains(id))
            .collect();
        assert!(next.len() <= 1, "tick {tick} admitted {next:?}");
        visited.extend(next);
        if tick == 39 {
            assert_eq!(visited, ids, "every domain is visited within one period");
        }
    }
    assert_eq!(visited, ids);
}

fn garrison_domain(w: &mut GameWorld, x: i32) -> EntityId {
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(x, 90, 4, 4),
            true,
        )
        .unwrap();
    let unit = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            WorldPosition::from_tile_center(TileCoord::new(x - 1, 92)).unwrap(),
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), unit, castle).unwrap();
    unit
}

#[test]
fn road_planning_yields_and_every_domain_finishes() {
    let (mut w, v, _) = setup();
    let mut ids = vec![v];
    for x in [140, 190] {
        ids.push(garrison_domain(&mut w, x));
    }
    advance(&mut w, 2);
    w.set_fog_disabled(true);
    let mut yielded = std::collections::BTreeSet::new();
    for tick in 20..40 {
        w.tick.0 = tick;
        w.advance_vassals(&mut Vec::new());
        yielded.extend(
            ids.iter()
                .copied()
                .filter(|&id| w.vassal_planning_pending(id)),
        );
    }
    assert_eq!(
        yielded.into_iter().collect::<Vec<_>>(),
        ids,
        "road searches must yield"
    );
    let mut ticks = 0;
    while ids.iter().any(|&id| w.vassal_planning_pending(id)) && ticks < 2_000 {
        w.tick.0 += 1;
        w.advance_vassals(&mut Vec::new());
        ticks += 1;
    }
    assert!(ticks > 0);
    for id in ids {
        assert!(!w.vassal_planning_pending(id), "no plan may starve: {id:?}");
        assert!(!w.vassal_network(id).is_empty());
    }
}

#[test]
fn pending_search_progress_is_hashed_deterministically() {
    let (mut a, v, _) = setup();
    let (mut b, _, _) = setup();
    for w in [&mut a, &mut b] {
        w.set_fog_disabled(true);
        w.tick.0 = 20;
        w.advance_vassals(&mut Vec::new());
    }
    assert!(a.vassal_planning_pending(v));
    assert_eq!(a.canonical_hash(), b.canonical_hash());
    a.advance_vassals(&mut Vec::new());
    assert_ne!(a.canonical_hash(), b.canonical_hash());
    b.advance_vassals(&mut Vec::new());
    assert_eq!(a.canonical_hash(), b.canonical_hash());
}

#[test]
fn vassal_caps_reserve_pause_and_completed_land_stops_spending() {
    let (mut w, v, _) = setup();
    // Explored ground carries the wall ring; under fog it is not planned.
    w.set_fog_disabled(true);
    for i in 0..25 {
        let pos = WorldPosition::from_tile_center(TileCoord::new(84 + i % 5, 96 + i / 5)).unwrap();
        w.spawn_villager(PlayerId(0), pos).unwrap();
        w.spawn_kind(PlayerId(0), UnitKind::Militia, pos).unwrap();
    }
    w.issue_vassal_development(PlayerId(0), v, false).unwrap();
    advance(&mut w, 40);
    let s = w.vassal_status(v).unwrap();
    assert_eq!(s.military, 20);
    assert!(
        w.economy.dispatch.sites.is_empty(),
        "pausing stops placement"
    );
    let ticks_per_minute = u64::from(w.config().tick_hz) * 60;
    // The Scout was alone for setup's two ticks before 25 Militia joined.
    let upkeep = |tick: u64| ((26 * tick - 50) / ticks_per_minute) as u32;
    let paid = upkeep(w.tick().0);
    let mut before = w.player_economy(PlayerId(0)).unwrap().stockpile;
    advance(&mut w, 40);
    before.food -= upkeep(w.tick().0) - paid;
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, before);
    w.issue_vassal_development(PlayerId(0), v, true).unwrap();
    advance(&mut w, 20);
    assert!(!w.economy.dispatch.sites.is_empty());
    for _ in 0..60_000 {
        w.advance();
        if w.vassal_status(v).unwrap().developed {
            break;
        }
    }
    assert!(
        w.vassal_status(v).unwrap().developed,
        "finite fortification plan must finish: {:?}; foundations {:?}; workers {:?}",
        w.vassal_status(v),
        w.buildings()
            .filter(|b| !b.is_complete())
            .map(|b| (b.kind, b.footprint, b.work_done, b.work_total))
            .collect::<Vec<_>>(),
        w.units()
            .filter(|u| {
                let t = u.position.tile_floor();
                (85..=88).contains(&t.x) && (96..=99).contains(&t.y)
            })
            .map(|u| (
                u.id,
                u.kind,
                u.position,
                u.task,
                u.moving,
                w.vassal_controller(u.id)
            ))
            .collect::<Vec<_>>()
    );
    assert!(w.economy.dispatch.sites.is_empty());
    let paid = upkeep(w.tick().0);
    let mut after = w.player_economy(PlayerId(0)).unwrap().stockpile;
    advance(&mut w, 100);
    after.food -= upkeep(w.tick().0) - paid;
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().stockpile, after);
    assert!(w.buildings().any(|b| b.kind.is_gate() && b.is_complete()));
    assert_eq!(
        w.issue_vassal_development(PlayerId(1), v, false),
        Err(EconomyError::NotOwner)
    );
}
