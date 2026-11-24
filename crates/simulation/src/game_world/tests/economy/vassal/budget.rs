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
