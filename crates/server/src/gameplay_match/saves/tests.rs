use super::*;
use aoe_map::{MAP_SCHEMA_VERSION, MapRequest, Ratio};

fn package() -> MapPackage {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package")
}

#[tokio::test]
async fn a_saved_match_resumes_with_the_same_seats_units_and_buildings() {
    let package = package();
    let directory = std::env::temp_dir().join(format!("aoe-saves-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let (service, _) = GameplayService::from_match_rules(
        package.clone(),
        None,
        &[true, false, false],
        &MatchRules::default(),
        &std::sync::atomic::AtomicUsize::new(0),
        &|| false,
    )
    .expect("match");
    for _ in 0..40 {
        service.tick().await;
    }
    let card = service
        .save_game(directory.clone(), "Round trip".to_owned())
        .await
        .expect("saved");
    assert_eq!(card.opponents, 2);
    assert!(card.bytes > 0 && card.units >= 12, "{card:?}");
    assert_eq!(list(&directory).len(), 1);

    let loaded = read_save(&directory, &card.id).expect("readable");
    let (resumed, report) =
        resume_service(package.clone(), None, &loaded, &|| false, None).expect("resumed");
    assert_eq!(report.units_lost + report.buildings_lost, 0, "{report:?}");
    assert_eq!(resumed.slot_infos().await.len(), 3);
    assert_eq!(
        resumed
            .slot_infos()
            .await
            .iter()
            .map(|s| (s.player, s.primary_unit_id, s.human))
            .collect::<Vec<_>>(),
        service
            .slot_infos()
            .await
            .iter()
            .map(|s| (s.player, s.primary_unit_id, s.human))
            .collect::<Vec<_>>()
    );
    let hash = package.content_hash;
    let (before, after) = (
        service.world.read().await.snapshot(hash).expect("snapshot"),
        resumed.world.read().await.snapshot(hash).expect("snapshot"),
    );
    assert_eq!(after.tick, before.tick);
    assert_eq!(after.players, before.players);
    assert_eq!(after.buildings, before.buildings);
    let ids = |s: &aoe_simulation::WorldSnapshot| {
        s.units
            .iter()
            .map(|u| (u.unit.id, u.unit.kind, u.unit.player, u.unit.hp))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&after), ids(&before));
    // The resumed match plays on.
    for _ in 0..40 {
        resumed.tick().await;
    }
    assert!(resumed.world.read().await.tick().0 > before.tick);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn save_ids_never_leave_the_save_directory() {
    let directory = std::env::temp_dir();
    for id in ["../x", "a/b", "", "a.b", "..", "x\0"] {
        assert!(read_save(&directory, id).is_err(), "{id:?}");
    }
}
