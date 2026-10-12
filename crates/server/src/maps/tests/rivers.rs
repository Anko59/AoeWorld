use super::*;
use std::{fs, path::Path as FsPath};

fn config(directory: &FsPath) -> crate::Config {
    crate::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.to_owned()),
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    }
}

// Deliberately publish self-consistent hostile bytes without the writer's self-check.
fn hostile_package(directory: &FsPath, fault: u8) -> MapPackage {
    let (base, _, elevation, mut hydrology, cover) = modeled_source_package();
    let mut environment = base.environment.clone();
    match fault {
        0 => hydrology.river_segments.push(aoe_map::RiverSegment {
            start: [0, 0],
            end: [67_108_864, 67_108_864],
            width_units: 768,
            station_start: 0,
            station_end: 1000,
        }),
        1 => hydrology.water_model = None,
        2 => environment.hydrology_evidence.as_mut().unwrap().water_model = None,
        _ => unreachable!(),
    }
    environment
        .hydrology_evidence
        .as_mut()
        .unwrap()
        .hydrology_page_root = ordered_hydrology_page_root(&[hydrology.clone()]).unwrap();
    let package = MapPackage::with_prepared_environment(
        base.generator_version,
        base.request,
        base.source_locks,
        base.projection,
        base.provenance,
        environment,
    )
    .unwrap();
    let root = directory.join("pages").join(package.content_hash_hex());
    for layer in ["elevation", "hydrology-evidence", "modern-land-cover"] {
        fs::create_dir_all(root.join(layer)).unwrap();
    }
    for page in [elevation, elevation_overview()] {
        fs::write(
            root.join(format!("elevation/{}-0-0.json", page.level)),
            serde_json::to_vec(&page).unwrap(),
        )
        .unwrap();
    }
    fs::write(
        root.join("hydrology-evidence/0-0-0.json"),
        serde_json::to_vec(&hydrology).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("modern-land-cover/0-0-0.json"),
        serde_json::to_vec(&cover).unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join(format!("{}.json", package.content_hash_hex())),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    package
}

fn reason(fault: u8) -> &'static str {
    if fault == 0 {
        "river segment 0 endpoint 1 axis 0 lies outside its page plus five-tile margin"
    } else {
        "hydrology page water-model presence does not match its index"
    }
}

#[test]
fn server_verification_rejects_self_consistent_out_of_margin_rivers_and_model_mismatch() {
    for fault in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let package = hostile_package(directory.path(), fault);
        let error = crate::map_store::verify_stored(directory.path(), &package)
            .expect_err("hostile package");
        assert!(error.to_string().contains(reason(fault)), "{error}");
        assert!(crate::map_store::load(Some(directory.path())).is_err());
        let error = crate::map_store::PageResidency::open(directory.path(), &package, &|| false)
            .expect_err("page decode must reject before any terrain query");
        assert!(error.to_string().contains(reason(fault)), "{error}");
    }
}

#[tokio::test]
async fn activation_rejects_out_of_margin_rivers_even_with_lake_evidence() {
    for fault in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let state =
            crate::AppState::new(&config(directory.path()), "river-activation-test").unwrap();
        let previous = state.gameplay.read().await.world_id();
        let generation = state.generation.load(Ordering::SeqCst);
        let package = hostile_package(directory.path(), fault);
        let result = super::super::activate_completed(state.clone(), package).await;
        let Err(error) = result else {
            panic!("hostile activation succeeded")
        };
        assert_eq!(error.0, StatusCode::NOT_FOUND);
        assert!(error.1.contains(reason(fault)), "{}", error.1);
        assert_eq!(state.gameplay.read().await.world_id(), previous);
        assert_eq!(state.generation.load(Ordering::SeqCst), generation);
        assert_eq!(state.page_residencies.write().await.len(), 0);
    }
}
