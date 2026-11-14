use crate::fixtures::{Fixture, Kind, check, discover};
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use std::{fs, path::Path};

#[test]
fn a_ship_runs_the_report_form_for_map_and_gameplay_changes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = crate::gates::registry::Registry::load(&root).unwrap();
    let gate = registry
        .gates
        .iter()
        .find(|gate| gate.id == "check-fixtures-report")
        .unwrap();
    assert_eq!(gate.command, "make check-fixtures-report");
    assert!(
        gate.cadences
            .contains(&crate::gates::registry::Cadence::Preflight)
    );
    assert!(!gate.long);
    // Only the code that can change a package or save format reruns it.
    let mut suites = gate.suites.clone();
    suites.sort();
    assert_eq!(suites, ["gameplay", "map"]);
}

fn sources(root: &Path, replay: &str, ai: &str, config: &str) {
    fs::create_dir_all(root.join("docs/scale")).unwrap();
    fs::create_dir_all(root.join("qa")).unwrap();
    fs::write(root.join("docs/scale/save-replay.md"), replay).unwrap();
    fs::write(root.join("docs/ai-scaling.md"), ai).unwrap();
    fs::write(root.join("qa/ai-scaling.json"), config).unwrap();
}

#[test]
fn fixtures_parse_documented_paths_and_workload_hashes_once() {
    let temp = tempfile::tempdir().unwrap();
    let hash = "a".repeat(64);
    let path = format!(".cache/dev-replay/packages/{hash}.json");
    sources(
        temp.path(),
        &format!(
            "`{path}`\n--save '.cache/dev-replay/packages/saves/war.save'\n`{path}`\n.cache/dev-replay/report.json\nold.save"
        ),
        &format!("--map-hash {hash}"),
        &format!(r#"{{"map_hash":"{hash}"}}"#),
    );
    let list = discover(temp.path()).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&Fixture {
        path: path.into(),
        kind: Kind::Package
    }));
    assert!(list.contains(&Fixture {
        path: ".cache/dev-replay/packages/saves/war.save".into(),
        kind: Kind::Save
    }));
}

#[test]
fn fixtures_follow_both_docs_and_config_when_their_hashes_differ() {
    let temp = tempfile::tempdir().unwrap();
    sources(
        temp.path(),
        "",
        &format!("--map-hash {}", "b".repeat(64)),
        &format!(r#"{{"map_hash":"{}"}}"#, "c".repeat(64)),
    );
    assert_eq!(discover(temp.path()).unwrap().len(), 2);
    fs::write(
        temp.path().join("qa/ai-scaling.json"),
        r#"{"map_hash":"bad"}"#,
    )
    .unwrap();
    assert!(
        discover(temp.path())
            .unwrap_err()
            .to_string()
            .contains("map_hash")
    );
}

#[test]
fn fixtures_find_make_assignments_and_equals_style_cli_arguments() {
    let temp = tempfile::tempdir().unwrap();
    let mekong = "a".repeat(64);
    let france = "b".repeat(64);
    sources(
        temp.path(),
        &format!(
            "make map-verify AOE_MAP_PACKAGE=.cache/dev-replay/packages/{mekong}.json\n--save=.cache/dev-replay/packages/saves/war.save"
        ),
        &format!("--map-hash={france}"),
        &format!(r#"{{"map_hash":"{france}"}}"#),
    );
    let fixtures = discover(temp.path()).unwrap();
    assert_eq!(fixtures.len(), 3, "{fixtures:?}");
}

#[test]
fn fixtures_report_ok_fail_skipped_and_continue_after_failure() {
    let temp = tempfile::tempdir().unwrap();
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), vec![]).unwrap();
    let directory = temp.path().join(".cache/dev-replay/packages");
    fs::create_dir_all(&directory).unwrap();
    let good = format!(
        ".cache/dev-replay/packages/{}.json",
        package.content_hash_hex()
    );
    fs::write(
        temp.path().join(&good),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    let bad = ".cache/dev-replay/packages/bad.json";
    fs::write(temp.path().join(bad), b"broken json").unwrap();
    let missing = ".cache/dev-replay/packages/missing.json";
    let fixtures = [bad, good.as_str(), missing].map(|path| Fixture {
        path: path.into(),
        kind: Kind::Package,
    });
    let mut output = Vec::new();
    assert!(!check(temp.path(), &fixtures, &mut output).unwrap());
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains(&format!("OK {good}")), "{text}");
    assert!(
        text.contains(&format!("FAIL {bad}: expected value")),
        "{text}"
    );
    assert!(text.contains("GENERATION_RECIPE_VERSION"), "{text}");
    assert!(text.contains(&format!("SKIPPED {missing}")), "{text}");
    assert!(check(temp.path(), &fixtures[1..], &mut Vec::new()).unwrap());
}

#[test]
fn fixtures_reject_an_unsupported_recipe_with_the_game_error() {
    let temp = tempfile::tempdir().unwrap();
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), vec![]).unwrap();
    let mut value = serde_json::to_value(package).unwrap();
    value["generation_recipe_version"] = 0.into();
    fs::write(
        temp.path().join("recipe.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let mut output = Vec::new();
    assert!(
        !check(
            temp.path(),
            &[Fixture {
                path: "recipe.json".into(),
                kind: Kind::Package
            }],
            &mut output
        )
        .unwrap()
    );
    let text = String::from_utf8(output).unwrap();
    assert!(
        text.contains("generation recipe version is unsupported"),
        "{text}"
    );
}

#[test]
fn fixtures_present_save_with_bad_snapshot_is_failure_not_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join(".cache/dev-replay/packages/saves");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("war.json"), r#"{"id":"war","name":"war","saved_at":0,"tick":0,"game_seconds":0,"map_hash":"unused","opponents":0,"units":0,"buildings":0,"bytes":1,"save_ms":0}"#).unwrap();
    fs::write(directory.join("war.save"), [255]).unwrap();
    let mut output = Vec::new();
    assert!(
        !check(
            temp.path(),
            &[Fixture {
                path: ".cache/dev-replay/packages/saves/war.save".into(),
                kind: Kind::Save
            }],
            &mut output
        )
        .unwrap()
    );
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("the saved game is unreadable:"), "{text}");
    assert!(
        text.contains("SAVE_VERSION") && text.contains("WORLD_SNAPSHOT_VERSION"),
        "{text}"
    );
}

#[test]
fn fixtures_missing_save_is_success_but_missing_card_is_failure() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = Fixture {
        path: "war.save".into(),
        kind: Kind::Save,
    };
    let mut output = Vec::new();
    assert!(check(temp.path(), std::slice::from_ref(&fixture), &mut output).unwrap());
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "SKIPPED war.save (missing)\n"
    );
    fs::write(temp.path().join("war.save"), [0]).unwrap();
    let mut output = Vec::new();
    assert!(!check(temp.path(), &[fixture], &mut output).unwrap());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("unknown saved game")
    );
}

#[test]
fn fixtures_package_identity_must_match_its_filename() {
    let temp = tempfile::tempdir().unwrap();
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), vec![]).unwrap();
    fs::write(
        temp.path().join("wrong.json"),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    let mut output = Vec::new();
    assert!(
        !check(
            temp.path(),
            &[Fixture {
                path: "wrong.json".into(),
                kind: Kind::Package
            }],
            &mut output
        )
        .unwrap()
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("package content hash does not match filename")
    );
}

#[test]
fn fixtures_load_a_small_save_and_reject_its_snapshot_version() {
    let temp = tempfile::tempdir().unwrap();
    let packages = temp.path().join(".cache/dev-replay/packages");
    let saves = packages.join("saves");
    fs::create_dir_all(&saves).unwrap();
    let package = MapPackage::new(
        MAP_SCHEMA_VERSION,
        MapRequest {
            requested_side_meters: 1_000,
            compression: aoe_map::Ratio::new(5, 1).unwrap(),
            ..MapRequest::default()
        },
        vec![],
    )
    .unwrap();
    let hash = package.content_hash_hex();
    fs::write(
        packages.join(format!("{hash}.json")),
        serde_json::to_vec(&package).unwrap(),
    )
    .unwrap();
    fs::write(saves.join("empty.json"), format!(r#"{{"id":"empty","name":"empty","saved_at":0,"tick":0,"game_seconds":0,"map_hash":"{hash}","opponents":0,"units":0,"buildings":0,"bytes":14,"save_ms":0}}"#)).unwrap();
    let world = aoe_simulation::GameWorld::new(Default::default()).unwrap();
    let version = u8::try_from(world.snapshot([0; 32]).unwrap().version).unwrap();
    // Minimal current postcard save: save header, zero seats, empty snapshot.
    // This is a synthetic current-format pin, never a compatibility reader.
    let mut bytes = [1, 0, version, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let fixture = Fixture {
        path: ".cache/dev-replay/packages/saves/empty.save".into(),
        kind: Kind::Save,
    };
    fs::write(saves.join("empty.save"), bytes).unwrap();
    let mut output = Vec::new();
    assert!(
        check(temp.path(), std::slice::from_ref(&fixture), &mut output).unwrap(),
        "{}",
        String::from_utf8_lossy(&output)
    );
    bytes[2] = 0;
    fs::write(saves.join("empty.save"), bytes).unwrap();
    let mut output = Vec::new();
    assert!(!check(temp.path(), &[fixture], &mut output).unwrap());
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("the snapshot is from an unsupported version")
    );
}
