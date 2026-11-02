//! A General's influence reaches 50 tiles: he commands no soldier and chooses
//! no training building beyond it.
use super::*;

fn tiles_from(w: &GameWorld, a: EntityId, b: EntityId) -> i64 {
    let (a, b) = (w.unit(a).unwrap().position, w.unit(b).unwrap().position);
    (i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)).isqrt() / 1024
}

#[test]
fn a_march_leads_only_soldiers_within_fifty_tiles() {
    let (mut w, _) = setup();
    let near = soldier(&mut w, UnitKind::Militia, 100, 100);
    let far = soldier(&mut w, UnitKind::Militia, 101, 100);
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    // The straggler walked off after adoption.
    let index = w.lookup[&far];
    w.units[index].state.position = at(100, 170);
    assert!(tiles_from(&w, far, g) > 50);
    w.issue_move(g, at(145, 103)).unwrap();
    decide(&mut w);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Marching);
    assert!(w.formation_destination_slot(near).is_some());
    assert!(
        w.formation_destination_slot(far).is_none(),
        "no order reaches a soldier 70 tiles away"
    );
    // Back inside the sphere he is commanded again.
    let index = w.lookup[&far];
    w.units[index].state.position = at(110, 120);
    w.economy.generals.armies.get_mut(&g).unwrap().routed = None;
    decide(&mut w);
    assert!(w.formation_destination_slot(far).is_some());
}

#[test]
fn training_uses_only_buildings_within_fifty_tiles() {
    let (mut w, _) = setup();
    let near = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(100, 80, 3, 3),
            true,
        )
        .unwrap();
    let far = w
        .insert_building(
            PlayerId(0),
            BuildingKind::ArcheryRange,
            aoe_core::TileRect::from_xywh(100, 190, 3, 3),
            true,
        )
        .unwrap();
    let g = soldier(&mut w, UnitKind::General, 105, 100);
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    for _ in 0..12 {
        decide(&mut w);
    }
    assert!(!w.building(near).unwrap().training_queue.is_empty());
    assert!(
        w.building(far).unwrap().training_queue.is_empty(),
        "an Archery Range 90 tiles away is out of reach"
    );
}
