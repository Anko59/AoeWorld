//! Review findings: unfinished roads stay planned, stone replaces palisade on
//! exposed sides, every gate joins the road network, far work points get
//! roads, and blocked sites stay deficits (the last lives in `plan.rs`).
use super::*;

fn roads(w: &mut GameWorld, v: EntityId) -> BTreeSet<TileCoord> {
    w.vassal_network(v)
        .into_iter()
        .filter(|n| n.0 == BuildingKind::Road)
        .map(|n| n.1)
        .collect()
}

#[test]
fn replanning_keeps_unfinished_road_foundations() {
    let (mut w, v, _) = setup();
    let first = *roads(&mut w, v).iter().next().expect("a planned road");
    w.insert_building(
        PlayerId(0),
        BuildingKind::Road,
        TileRect::from_xywh(first.x, first.y, 1, 1),
        false,
    )
    .unwrap();
    w.tick.0 += 2_000; // past the re-plan period
    assert!(
        roads(&mut w, v).contains(&first),
        "the foundation must remain a deficit"
    );
}

#[test]
fn stone_replaces_palisade_on_an_exposed_side() {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    let plan = w.vassal_fortification(v);
    let stone_wall = plan
        .iter()
        .find(|(k, _)| *k == BuildingKind::StoneWall)
        .unwrap()
        .1;
    let stone_gate = plan
        .iter()
        .find(|(k, _)| *k == BuildingKind::StoneGate)
        .unwrap()
        .1;
    w.insert_building(PlayerId(0), BuildingKind::PalisadeWall, stone_wall, true)
        .unwrap();
    w.insert_building(PlayerId(0), BuildingKind::PalisadeGate, stone_gate, true)
        .unwrap();
    for i in 0..8 {
        let at = WorldPosition::from_tile_center(TileCoord::new(86 + i, 97)).unwrap();
        w.spawn_villager(PlayerId(0), at).unwrap();
    }
    let kind_at = |w: &GameWorld, r: TileRect| {
        w.building_at(r.min)
            .and_then(|id| w.building(id))
            .map(|b| b.kind)
    };
    for _ in 0..60_000 {
        w.advance();
        if kind_at(&w, stone_wall) == Some(BuildingKind::StoneWall)
            && kind_at(&w, stone_gate) == Some(BuildingKind::StoneGate)
        {
            return;
        }
    }
    panic!(
        "palisade stayed: wall {:?} gate {:?}",
        kind_at(&w, stone_wall),
        kind_at(&w, stone_gate)
    );
}

#[test]
fn every_gate_joins_the_road_network_on_both_sides() {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    let net = roads(&mut w, v);
    let gates: Vec<_> = w
        .vassal_fortification(v)
        .into_iter()
        .filter(|(k, _)| k.is_gate())
        .collect();
    assert!(gates.len() >= 8, "{} gates", gates.len());
    for (_, r) in gates {
        let sides = if r.width() > r.height() {
            [(r.min.y - 1), r.max.y]
                .map(|y| (r.min.x - 1..=r.max.x).any(|x| net.contains(&TileCoord::new(x, y))))
        } else {
            [(r.min.x - 1), r.max.x]
                .map(|x| (r.min.y - 1..=r.max.y).any(|y| net.contains(&TileCoord::new(x, y))))
        };
        assert_eq!(
            sides,
            [true, true],
            "gate {r:?} is not joined on both sides"
        );
    }
}

#[test]
fn a_town_center_outside_the_wall_square_but_in_the_domain_gets_a_road() {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    w.place_town_center(PlayerId(0), TileCoord::new(172, 90))
        .unwrap();
    assert!(
        roads(&mut w, v).iter().any(|t| t.x >= 170),
        "no road beyond the wall toward the far Town Center"
    );
}

#[test]
fn the_castle_trunk_serves_existing_and_planned_production() {
    let (mut w, v, castle) = setup();
    w.set_fog_disabled(true);
    let workshop = TileRect::from_xywh(118, 121, 3, 3);
    w.insert_building(PlayerId(0), BuildingKind::SiegeWorkshop, workshop, true)
        .unwrap();

    let fort = w.vassal_fortification(v);
    let mut anchors: Vec<_> = fort
        .iter()
        .filter(|(kind, _)| {
            matches!(
                kind,
                BuildingKind::Barracks | BuildingKind::ArcheryRange | BuildingKind::Stable
            )
        })
        .copied()
        .collect();
    assert_eq!(anchors.len(), 3);
    anchors.push((BuildingKind::SiegeWorkshop, workshop));
    let mut tiles = roads(&mut w, v);
    let castle = w.building(castle).unwrap().footprint;
    let entrance = cardinal_edge(castle)
        .find(|tile| tiles.remove(tile))
        .expect("the trunk reaches the Castle");
    let mut visit = vec![entrance];
    let mut connected = BTreeSet::new();
    while let Some(tile) = visit.pop() {
        connected.insert(tile);
        visit.extend(
            cardinal_edge(TileRect::from_xywh(tile.x, tile.y, 1, 1)).filter(|t| tiles.remove(t)),
        );
    }
    eprintln!(
        "castle trunk: {} connected road tiles for {} production sites",
        connected.len(),
        anchors.len()
    );
    for (kind, footprint) in anchors {
        assert!(
            cardinal_edge(footprint).any(|t| connected.contains(&t)),
            "{kind:?} at {footprint:?} has no continuous road to the Castle"
        );
    }
}

fn cardinal_edge(r: TileRect) -> impl Iterator<Item = TileCoord> {
    (r.min.x..r.max.x)
        .flat_map(move |x| [TileCoord::new(x, r.min.y - 1), TileCoord::new(x, r.max.y)])
        .chain(
            (r.min.y..r.max.y)
                .flat_map(move |y| [TileCoord::new(r.min.x - 1, y), TileCoord::new(r.max.x, y)]),
        )
}
