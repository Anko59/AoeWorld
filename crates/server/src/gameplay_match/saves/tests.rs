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

/// A save whose body this build can no longer parse must still say it is from
/// another version. #828 added `FormationRecord::pace_limit` without bumping a
/// version, so every earlier save failed as a raw postcard decoding error.
#[test]
fn a_save_from_an_earlier_format_reports_its_version_not_a_decoding_error() {
    #[derive(serde::Serialize)]
    struct OldSaveFile {
        version: u16,
        seats: Vec<SaveSeat>,
        // A body this build cannot parse as a `WorldSnapshot`.
        world: u8,
    }
    let directory = std::env::temp_dir().join(format!("aoe-oldsave-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("directory");
    let id = "1700000000-old";
    let bytes = postcard::to_allocvec(&OldSaveFile {
        version: SAVE_VERSION - 1,
        seats: Vec::new(),
        world: 0,
    })
    .expect("encode");
    std::fs::write(directory.join(format!("{id}.save")), bytes).expect("save");
    std::fs::write(
        directory.join(format!("{id}.json")),
        serde_json::to_vec(&SaveCard {
            id: id.to_owned(),
            name: "old".to_owned(),
            saved_at: 0,
            tick: 0,
            game_seconds: 0,
            map_hash: String::new(),
            opponents: 0,
            units: 0,
            buildings: 0,
            bytes: 0,
            save_ms: 0,
        })
        .expect("card"),
    )
    .expect("card");
    let message = match read_save(&directory, id) {
        Err((_, message)) => message,
        Ok(_) => panic!("an earlier format must be refused"),
    };
    assert_eq!(message, "the saved game is from another version");
    let _ = std::fs::remove_dir_all(&directory);
}

/// `read_save` takes the version from the encoding's first field. Pin that on a
/// real `SaveFile`, not on a hand-built copy of its assumed layout: with nine
/// seats, a reordered struct would put a seat count (or the snapshot's own
/// version) where the version is read, and this test would fail.
#[test]
fn a_real_save_encodes_its_version_where_the_reader_looks_for_it() {
    let world = aoe_simulation::GameWorld::new(
        aoe_core::WorldConfig::new(64, 64, aoe_core::Seed(3)).expect("config"),
    )
    .expect("world");
    let encode = |version: u16| {
        let seats = (0..9)
            .map(|id| SaveSeat {
                primary_unit_id: id,
                human: id == 0,
            })
            .collect();
        let world = world.snapshot([0; 32]).expect("snapshot");
        postcard::to_allocvec(&SaveFile {
            version,
            seats,
            world,
        })
        .expect("encode")
    };
    let current = encode(SAVE_VERSION);
    let (header, _) = postcard::take_from_bytes::<SaveVersion>(&current).expect("header");
    assert_eq!(header.version, SAVE_VERSION);
    // A save from another build is refused by name, whatever its body holds.
    let directory = std::env::temp_dir().join(format!("aoe-saveorder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("directory");
    let id = "1700000000-next";
    std::fs::write(
        directory.join(format!("{id}.save")),
        encode(SAVE_VERSION + 1),
    )
    .expect("save");
    std::fs::write(
        directory.join(format!("{id}.json")),
        serde_json::to_vec(&SaveCard {
            id: id.to_owned(),
            name: "next".to_owned(),
            saved_at: 0,
            tick: 0,
            game_seconds: 0,
            map_hash: String::new(),
            opponents: 0,
            units: 0,
            buildings: 0,
            bytes: 0,
            save_ms: 0,
        })
        .expect("card"),
    )
    .expect("card");
    let message = match read_save(&directory, id) {
        Err((_, message)) => message,
        Ok(_) => panic!("a save from another version must be refused"),
    };
    assert_eq!(message, "the saved game is from another version");
    let _ = std::fs::remove_dir_all(&directory);
}
