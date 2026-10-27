use super::*;

#[test]
fn game_paths_and_non_game_paths_route_to_the_configured_workstreams() {
    let config = Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    for path in [
        "crates/simulation/src/lib.rs",
        "crates/core/src/lib.rs",
        "./crates/client/src/lib.rs",
        "crates/scenario/src/lib.rs",
        "crates/protocol/src/lib.rs",
        "crates/geodata/src/lib.rs",
        "crates/server/src/lib.rs",
        "crates/client/src/lib.rs",
        "crates/rendering/src/lib.rs",
        "crates/map/src/lib.rs",
        "crates/assets/src/lib.rs",
        "crates/sdk/src/lib.rs",
        "crates/sdk-python/src/lib.rs",
        "ai/model.py",
        "browser/tests/village/commands.play.spec.ts",
        "web/interface/hud.js",
    ] {
        assert_eq!(config.review(path), 575, "{path}");
    }
    for path in [
        "crates/harness/src/main.rs",
        "gates/registry.json",
        ".github/workflows/ci.yml",
        "docs/issues.md",
        "crates/serverish/file.rs",
        "browser/playwright.config.ts",
        "crates/server/README.md",
        "ai/AGENTS.md",
        "web/dev/tools.js",
        "browser/tests/tooling/quarantine-reporter.spec.ts",
        "browser/tests/showcase/readiness.spec.ts",
    ] {
        assert_eq!(config.review(path), 576, "{path}");
    }
    assert_eq!(config.nightly, 574);
}

#[test]
fn invalid_destinations_name_the_config_file() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("gates")).unwrap();
    for config in [
        r#"{"game_review":0,"other_review":576,"nightly":574}"#,
        r#"{"game_review":575,"other_review":575,"nightly":574}"#,
        r#"{"game_review":575,"other_review":576,"nightly":574,"extra":1}"#,
    ] {
        std::fs::write(temp.path().join("gates/issues.json"), config).unwrap();
        let error = match Config::load(temp.path()) {
            Ok(_) => panic!("accepted {config}"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("gates/issues.json"));
    }
}
