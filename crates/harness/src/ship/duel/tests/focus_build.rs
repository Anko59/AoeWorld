use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

#[test]
fn focused_browser_build_generates_the_diagnostics_bundle() {
    const CHILD: &str = "AOE_FOCUSED_BUILD_FIXTURE";
    if let Some(root) = std::env::var_os(CHILD) {
        super::browser_build(Path::new(&root)).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/client")).unwrap();
    fs::write(
        root.join("crates/client/Cargo.toml"),
        "[package]\nname = \"aoe-client\"\n[features]\ndiagnostics = []\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("crates/rendering/src/web")).unwrap();
    fs::write(
        root.join("crates/rendering/src/web/shaders.js"),
        "runtime shader dependency",
    )
    .unwrap();
    for program in ["cargo", "wasm-bindgen"] {
        let path = root.join(program);
        fs::write(&path, "#!/bin/sh\nprintf '%s %s\\n' \"$0\" \"$*\" >> \"$AOE_FOCUSED_BUILD_FIXTURE/commands\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ship::duel::focus::build_tests::focused_browser_build_generates_the_diagnostics_bundle", "--nocapture"])
        .env(CHILD, root)
        .env("PATH", format!("{}:{}", root.display(), std::env::var("PATH").unwrap()))
        .output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let commands = fs::read_to_string(root.join("commands")).unwrap();
    let lines: Vec<_> = commands.lines().collect();
    let diagnostics = lines
        .iter()
        .position(|line| line.contains("--out-dir web/diagnostics-pkg"))
        .expect("focused E2E builds must serve diagnostics.html on a fresh tree");
    assert!(lines[diagnostics - 1].contains("--features diagnostics"));
    assert!(lines.iter().any(|line| line.contains("--out-dir web/pkg")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("assets browser-shaders"))
    );
}
