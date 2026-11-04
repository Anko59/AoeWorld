use super::manager::{active, pass};
use super::*;
use crate::StewardSettings;

#[test]
fn leaving_releases_the_town_center_and_workers_and_a_distant_town_center_takes_over() {
    let (mut w, first, steward) = active();
    let settings = StewardSettings {
        weights: [0, 100, 0, 0],
        cap: 60,
        training: false,
    };
    w.issue_steward_settings(PlayerId(0), steward, settings)
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
    assert_eq!(w.steward_controller(old_worker), Some(steward));
    let second = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(210, 90, 4, 4),
            true,
        )
        .unwrap();
    let new_worker = farmer(&mut w, 220);
    w.issue_ungarrison(PlayerId(0), first, Some(steward))
        .unwrap();
    assert_eq!(
        w.steward_town_center(steward),
        None,
        "a roaming Steward has no Town Center"
    );
    assert_eq!(w.steward_controller(old_worker), None);
    pass(&mut w);
    assert_eq!(w.steward_work().domains, 0);
    assert_eq!(w.steward_controller(new_worker), None);
    // Leaving also frees the original Town Center's production reservation.
    w.issue_train(PlayerId(0), first, UnitKind::Steward)
        .unwrap();
    w.issue_cancel_training(PlayerId(0), first).unwrap();
    let beyond_birth_radius = WorldPosition::from_tile_center(TileCoord::new(208, 92)).unwrap();
    assert_eq!(w.issue_move(steward, beyond_birth_radius), Ok(true));
    w.issue_garrison(PlayerId(0), steward, second).unwrap();
    for _ in 0..4_000 {
        w.advance();
        if w.unit(steward).is_none() {
            break;
        }
    }
    assert!(
        w.unit(steward).is_none(),
        "Steward must enter the distant Town Center"
    );
    assert_eq!(w.steward_town_center(steward), Some(second));
    assert!(w.building(first).unwrap().garrison.is_empty());
    pass(&mut w);
    assert_eq!(w.steward_controller(old_worker), None);
    assert_eq!(w.steward_controller(new_worker), Some(steward));
    assert_eq!(
        w.steward_settings(steward),
        Some(settings),
        "settings travel with the unit"
    );
}

#[test]
fn a_town_center_admits_only_one_steward_including_arrivals_and_training() {
    let (mut w, first, a) = active();
    let second = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(110, 90, 4, 4),
            true,
        )
        .unwrap();
    let b = trained(&mut w, second);
    assert_eq!(
        w.steward_town_center(b),
        None,
        "birth does not reserve a Town Center"
    );
    w.issue_ungarrison(PlayerId(0), first, Some(a)).unwrap();
    w.issue_garrison(PlayerId(0), a, second).unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), b, second),
        Err(EconomyError::StewardExists)
    );
    assert_eq!(
        w.issue_train(PlayerId(0), second, UnitKind::Steward),
        Err(EconomyError::StewardExists)
    );
    assert_eq!(
        w.canonical_hash(),
        hash,
        "refused arrivals must preserve orders and costs"
    );
    // A manual override cancels the pending arrival and frees the Town Center.
    w.issue_move(
        a,
        WorldPosition::from_tile_center(TileCoord::new(80, 85)).unwrap(),
    )
    .unwrap();
    w.issue_train(PlayerId(0), second, UnitKind::Steward)
        .unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), a, second),
        Err(EconomyError::StewardExists)
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
        Err(EconomyError::StewardExists)
    );
    assert_eq!(w.canonical_hash(), hash);
}

#[test]
fn destruction_releases_a_surviving_steward_to_another_town_center() {
    let (mut w, town_center, steward) = active();
    let settings = StewardSettings {
        weights: [0, 0, 100, 0],
        cap: 42,
        training: false,
    };
    w.issue_steward_settings(PlayerId(0), steward, settings)
        .unwrap();
    let other = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(210, 90, 4, 4),
            true,
        )
        .unwrap();
    let foreign = w
        .insert_building(
            PlayerId(1),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(140, 40, 4, 4),
            true,
        )
        .unwrap();
    w.destroy_building(town_center);
    assert!(w.unit(steward).is_some_and(|u| u.hp > 0));
    assert_eq!(w.steward_town_center(steward), None);
    pass(&mut w);
    assert_eq!(w.steward_work().domains, 0);
    let tc = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            aoe_core::TileRect::from_xywh(30, 30, 4, 4),
            true,
        )
        .unwrap();
    let hash = w.canonical_hash();
    assert_eq!(
        w.issue_garrison(PlayerId(0), steward, foreign),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        w.issue_garrison(PlayerId(0), steward, tc),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(w.canonical_hash(), hash);
    w.issue_garrison(PlayerId(0), steward, other).unwrap();
    for _ in 0..4_000 {
        w.advance();
        if w.unit(steward).is_none() {
            break;
        }
    }
    assert!(w.unit(steward).is_none());
    assert_eq!(w.steward_town_center(steward), Some(other));
    assert_eq!(w.steward_settings(steward), Some(settings));
}
