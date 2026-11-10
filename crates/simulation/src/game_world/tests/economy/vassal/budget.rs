use super::*;

#[test]
fn road_planning_yields_and_every_domain_finishes() {
    let (mut w, v, _) = setup();
    let mut ids = vec![v];
    for x in [140, 190] {
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
        ids.push(unit);
    }
    advance(&mut w, 2);
    w.set_fog_disabled(true);
    w.tick.0 = 19;
    w.advance_vassals(&mut Vec::new());
    w.tick.0 = 20;
    w.advance_vassals(&mut Vec::new());
    assert!(
        ids.iter().all(|&id| w.vassal_planning_pending(id)),
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
