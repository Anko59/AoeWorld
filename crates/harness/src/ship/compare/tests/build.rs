use crate::ship::compare::build::build_app;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

#[test]
fn fresh_comparison_and_shared_server_builds_include_the_diagnostics_bundle() {
    const CHILD: &str = "AOE_BUNDLE_BUILD_FIXTURE";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = Path::new(&root);
        build_app(root, &root.join("target"), root).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let docker = root.join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$AOE_BUNDLE_BUILD_FIXTURE/commands\"\n",
    )
    .unwrap();
    fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ship::compare::tests::build::fresh_comparison_and_shared_server_builds_include_the_diagnostics_bundle", "--nocapture"])
        .env(CHILD, root)
        .env("PATH", format!("{}:{}", root.display(), std::env::var("PATH").unwrap()))
        .output().unwrap();
    assert!(result.status.success(), "{result:?}");
    let commands = fs::read_to_string(root.join("commands")).unwrap();
    let lines: Vec<_> = commands.lines().collect();
    let diagnostics = lines
        .iter()
        .position(|line| line.contains("--out-dir web/diagnostics-pkg"))
        .expect("fresh builds must generate the bundle imported by diagnostics.html");
    assert!(lines[diagnostics - 1].contains("--features diagnostics"));
    assert!(lines.iter().any(|line| line.contains("--out-dir web/pkg")));
    let html = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/diagnostics.html"),
    )
    .unwrap();
    assert!(html.contains("/diagnostics-pkg/aoe_client.js"));
}
