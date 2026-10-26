use crate::ship::compare::build::build_app;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

const CHILD: &str = "AOE_BUNDLE_BUILD_FIXTURE";

/// Runs `build_app` in a child test with a recording `docker`, over a tree whose
/// client manifest is `manifest`, and returns the recorded command lines.
fn recorded_build(test: &str, manifest: &str, external_shaders: bool) -> Vec<String> {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/client")).unwrap();
    fs::write(root.join("crates/client/Cargo.toml"), manifest).unwrap();
    if external_shaders {
        fs::create_dir_all(root.join("crates/rendering/src/web")).unwrap();
        fs::write(
            root.join("crates/rendering/src/web/shaders.js"),
            "runtime shader dependency",
        )
        .unwrap();
    }
    let docker = root.join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$AOE_BUNDLE_BUILD_FIXTURE/commands\"\n",
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, root)
        .env(
            "PATH",
            format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let commands = fs::read_to_string(root.join("commands")).unwrap();
    commands.lines().map(str::to_owned).collect()
}

fn run_child() -> bool {
    let Some(root) = std::env::var_os(CHILD) else {
        return false;
    };
    let root = Path::new(&root);
    build_app(root, &root.join("target"), root).unwrap();
    true
}

#[test]
fn fresh_comparison_and_shared_server_builds_include_the_diagnostics_bundle() {
    if run_child() {
        return;
    }
    let lines = recorded_build(
        "ship::compare::tests::build::fresh_comparison_and_shared_server_builds_include_the_diagnostics_bundle",
        "[package]\nname = \"aoe-client\"\n[features]\ndiagnostics = []\n",
        true,
    );
    let diagnostics = lines
        .iter()
        .position(|line| line.contains("--out-dir web/diagnostics-pkg"))
        .expect("fresh builds must generate the bundle imported by diagnostics.html");
    assert!(lines[diagnostics - 1].contains("--features diagnostics"));
    assert!(lines.iter().any(|line| line.contains("--out-dir web/pkg")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("assets browser-shaders"))
    );
    let html = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/diagnostics.html"),
    )
    .unwrap();
    assert!(html.contains("/diagnostics-pkg/aoe_client.js"));
}

#[test]
fn a_merge_base_without_the_diagnostics_feature_builds_only_gameplay() {
    if run_child() {
        return;
    }
    let lines = recorded_build(
        "ship::compare::tests::build::a_merge_base_without_the_diagnostics_feature_builds_only_gameplay",
        "[package]\nname = \"aoe-client\"\n",
        false,
    );
    assert!(lines.iter().any(|line| line.contains("--out-dir web/pkg")));
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("--features diagnostics"))
    );
    assert!(!lines.iter().any(|line| line.contains("diagnostics-pkg")));
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("assets browser-shaders"))
    );
}
