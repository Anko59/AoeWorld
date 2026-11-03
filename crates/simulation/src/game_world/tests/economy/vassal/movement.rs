use super::manager::{active, pass};
use super::*;
use crate::VassalSettings;

#[test]
fn leaving_releases_the_castle_and_workers_and_a_distant_castle_takes_over() {
    let (mut w, first, vassal) = active();
    let settings = VassalSettings {
        weights: [0, 100, 0, 0],
        cap: 60,
        training: false,
    };
    w.issue_vassal_settings(PlayerId(0), vassal, settings)
        .unwrap();
    let farmer = |w: &mut GameWorld, x| {
        let farm = w
            .insert_building(
                PlayerId(0),
                BuildingKind::Farm,
                aoe_core::TileRect::from_xywh(x, 104, 3, 3),
                true,
            )
            .unwrap();
        let worker = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap(),
            )
            .unwrap();
        w.issue_farm(PlayerId(0), worker, farm).unwrap();
        worker
    };
    let old_worker = farmer(&mut w, 100);
    pass(&mut w);
    assert_eq!(w.vassal_controller(old_worker), Some(vassal));
    let second = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(210, 90, 4, 4),
            true,
        )
        .unwrap();
    let new_worker = farmer(&mut w, 220);
    w.issue_ungarrison(PlayerId(0), first, Some(vassal))
        .unwrap();
    assert_eq!(
        w.vassal_castle(vassal),
        None,
        "a roaming Vassal has no Castle"
    );
    assert_eq!(w.vassal_controller(old_worker), None);
    pass(&mut w);
    assert_eq!(w.vassal_work().domains, 0);
    assert_eq!(w.vassal_controller(new_worker), None);
    // Leaving also frees the original Castle's production reservation.
    w.issue_train(PlayerId(0), first, UnitKind::Vassal).unwrap();
    w.issue_cancel_training(PlayerId(0), first).unwrap();
    let beyond_birth_radius = WorldPosition::from_tile_center(TileCoord::new(208, 92)).unwrap();
    assert_eq!(w.issue_move(vassal, beyond_birth_radius), Ok(true));
    w.issue_garrison(PlayerId(0), vassal, second).unwrap();
    for _ in 0..4_000 {
        w.advance();
        if w.unit(vassal).is_none() {
            break;
        }
    }
    assert!(
        w.unit(vassal).is_none(),
        "Vassal must enter the distant Castle"
    );
    assert_eq!(w.vassal_castle(vassal), Some(second));
    assert!(w.building(first).unwrap().garrison.is_empty());
    pass(&mut w);
    assert_eq!(w.vassal_controller(old_worker), None);
    assert_eq!(w.vassal_controller(new_worker), Some(vassal));
    assert_eq!(
        w.vassal_settings(vassal),
        Some(settings),
        "settings travel with the unit"
    );
}

#[test]
fn a_castle_admits_only_one_vassal_including_arrivals_and_training() {
    let (mut w, first, a) = active();
    let second = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(110, 90, 4, 4),
            true,
        )
        .unwrap();
    let b = trained(&mut w, second);
    assert_eq!(w.vassal_castle(b), None, "birth does not reserve a Castle");
    w.issue_ungarrison(PlayerId(0), first, Some(a)).unwrap();
    w.issue_garrison(PlayerId(0), a, second).unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), b, second),
        Err(EconomyError::VassalExists)
    );
    assert_eq!(
        w.issue_train(PlayerId(0), second, UnitKind::Vassal),
        Err(EconomyError::VassalExists)
    );
    assert_eq!(
        w.canonical_hash(),
        hash,
        "refused arrivals must preserve orders and costs"
    );
    // A manual override cancels the pending arrival and frees the Castle.
    w.issue_move(
        a,
        WorldPosition::from_tile_center(TileCoord::new(80, 85)).unwrap(),
    )
    .unwrap();
    w.issue_train(PlayerId(0), second, UnitKind::Vassal)
        .unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), a, second),
        Err(EconomyError::VassalExists)
    );
    assert_eq!(w.canonical_hash(), hash);
    w.issue_cancel_training(PlayerId(0), second).unwrap();
    w.issue_garrison(PlayerId(0), b, second).unwrap();
    advance(&mut w, 40);
    assert!(
        w.building(second)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == b)
    );
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), a, second),
        Err(EconomyError::VassalExists)
    );
    assert_eq!(w.canonical_hash(), hash);
}

#[test]
fn destruction_releases_a_surviving_vassal_to_another_castle() {
    let (mut w, castle, vassal) = active();
    let settings = VassalSettings {
        weights: [0, 0, 100, 0],
        cap: 42,
        training: false,
    };
    w.issue_vassal_settings(PlayerId(0), vassal, settings)
        .unwrap();
    let other = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(210, 90, 4, 4),
            true,
        )
        .unwrap();
    let foreign = w
        .insert_building(
            PlayerId(1),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(140, 40, 4, 4),
            true,
        )
        .unwrap();
    w.destroy_building(castle);
    assert!(w.unit(vassal).is_some_and(|u| u.hp > 0));
    assert_eq!(w.vassal_castle(vassal), None);
    pass(&mut w);
    assert_eq!(w.vassal_work().domains, 0);
    let tc = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .id;
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), vassal, foreign),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        w.issue_garrison(PlayerId(0), vassal, tc),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(w.canonical_hash(), hash);
    w.issue_garrison(PlayerId(0), vassal, other).unwrap();
    for _ in 0..4_000 {
        w.advance();
        if w.unit(vassal).is_none() {
            break;
        }
    }
    assert!(w.unit(vassal).is_none());
    assert_eq!(w.vassal_castle(vassal), Some(other));
    assert_eq!(w.vassal_settings(vassal), Some(settings));
}
