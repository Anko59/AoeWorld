use super::super::{save, tests::options};
use clap::Parser;
use serde_json::json;

#[test]
fn save_arguments_accept_a_path_and_reject_new_world_options() {
    let parsed = options(&[
        "--save",
        ".cache/owner.save",
        "--minutes",
        "10",
        "--port",
        "8160",
    ]);
    assert_eq!(parsed.save.unwrap().to_str(), Some(".cache/owner.save"));
    for extra in [
        vec!["--late-game"],
        vec!["--march-probe"],
        vec!["--browser"],
        vec!["--players", "4"],
        vec!["--humans", "0"],
        vec!["--hosts", "8"],
        vec!["--map", "flat"],
        vec!["--map-hash", "abc"],
        vec!["--tiles", "500"],
        vec!["--seed", "2"],
        vec!["--start-villagers", "3"],
        vec!["--start-militia", "2"],
    ] {
        let mut args = vec!["scale-match", "--save", "owner.save"];
        args.extend(extra);
        assert!(super::super::tests::Cli::try_parse_from(args).is_err());
    }
    assert!(super::super::tests::Cli::try_parse_from(["scale-match", "--save"]).is_err());
}

#[test]
fn save_preparation_requires_the_card_package_and_copies_both_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("owner.save");
    std::fs::write(&source, b"saved-world").unwrap();
    let packages = temp.path().join("packages");
    std::fs::create_dir(&packages).unwrap();
    let mut opts = options(&[
        "--save",
        source.to_str().unwrap(),
        "--package-directory",
        packages.to_str().unwrap(),
    ]);
    assert!(
        save::prepare(&mut opts)
            .unwrap_err()
            .to_string()
            .contains("card")
    );
    let package = aoe_map::MapPackage::new(1, aoe_map::MapRequest::default(), vec![]).unwrap();
    let hash = package.content_hash_hex();
    let card = json!({"id":"owner", "map_hash":hash, "opponents":99, "tick":430300, "units":9395});
    std::fs::write(
        source.with_extension("json"),
        serde_json::to_vec(&card).unwrap(),
    )
    .unwrap();
    let error = save::prepare(&mut opts).unwrap_err().to_string();
    assert!(
        error.contains(&hash) && error.contains("missing map package"),
        "{error}"
    );
    assert!(!packages.join("saves/owner.save").exists());
    std::fs::write(
        packages.join(format!("{hash}.json")),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    let prepared = save::prepare(&mut opts).unwrap().unwrap();
    assert_eq!(prepared["id"], "owner");
    assert_eq!(opts.players, 100);
    assert_eq!(opts.hosts, 8);
    assert_eq!(
        std::fs::read(packages.join("saves/owner.save")).unwrap(),
        b"saved-world"
    );
    assert_eq!(
        std::fs::read(source.with_extension("json")).unwrap(),
        std::fs::read(packages.join("saves/owner.json")).unwrap()
    );
    // Repeating with the copy already in the save directory is harmless.
    opts.save = Some(packages.join("saves/owner.save"));
    assert!(save::prepare(&mut opts).unwrap().is_some());
    opts.save = Some(source.clone());
    std::fs::write(source.with_extension("json"), b"{\"id\":\"../bad\"}").unwrap();
    assert!(save::prepare(&mut opts).is_err());
}

#[test]
fn phase_sampling_retains_every_exposed_phase() {
    let phases = json!({"total":{"p50_us":3000,"p95_us":6000},
        "new_phase":{"p50_us":1250,"p95_us":2500}});
    let row = super::super::report::phase_minute(Some(json!({"phases":phases})), None, 60.0);
    assert_eq!(row["new_phase"], json!([1.25, 2.5]));
    assert_eq!(row["total"], json!([3.0, 6.0]));
}

#[test]
fn minute_windows_use_observed_time_and_keep_every_tick_once() {
    let mut minutes = super::super::report::Minutes::default();
    let mut rows: Vec<_> = (0..600)
        .map(|tick| super::super::tests::row(tick, 40_000, 30_000, 100, false))
        .collect();
    minutes.record(&rows, 60.0);
    rows.extend((600..900).map(|tick| super::super::tests::row(tick, 80_000, 70_000, 99, true)));
    minutes.record(&rows, 120.0);
    assert_eq!(minutes.windows.len(), 2);
    assert_eq!(minutes.windows[0]["ticks_per_second"], 10.0);
    assert_eq!(minutes.windows[1]["ticks_per_second"], 5.0);
    assert_eq!(minutes.windows[1]["missed_ticks"], 300);
    assert_eq!(minutes.windows[1]["units_last"], 99);
    assert_eq!(
        minutes
            .windows
            .iter()
            .map(|w| w["ticks"].as_u64().unwrap())
            .sum::<u64>(),
        900
    );
}
