use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn command(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("Git command");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("Git output")
        .trim()
        .to_owned()
}

fn hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn fixture(root: &Path, revision: &str, tree: &str) {
    let release = root.join("reports/release").join(revision);
    fs::create_dir_all(&release).expect("release directory");
    let e2e = serde_json::to_vec(&serde_json::json!({"revision": revision, "result": "PASS"}))
        .expect("E2E JSON");
    let perf = serde_json::to_vec(
        &serde_json::json!({"revision": revision, "dirty": false, "verdict": "PASS"}),
    )
    .expect("performance JSON");
    fs::write(release.join("e2e.json"), &e2e).expect("E2E evidence");
    fs::write(release.join("perf.json"), &perf).expect("performance evidence");
    fs::write(release.join("bundle.tar"), b"synthetic bundle").expect("bundle");
    let mut sboms = serde_json::Map::new();
    for name in ["server.spdx.json", "browser.spdx.json", "bundle.spdx.json"] {
        let bytes = b"{\"spdxVersion\":\"SPDX-2.3\"}";
        fs::write(release.join(name), bytes).expect("SBOM");
        sboms.insert(name.into(), hash(bytes).into());
    }
    let manifest = serde_json::json!({
        "version": 1,
        "source_commit": revision,
        "source_tree": tree,
        "server_image": format!("ghcr.io/anko59/aoeworld-server@sha256:{}", "b".repeat(64)),
        "browser_image": format!("ghcr.io/anko59/aoeworld-browser@sha256:{}", "c".repeat(64)),
        "bundle_hash": "synthetic bundle content hash",
        "bundle_archive_hash": hash(b"synthetic bundle"),
        "protocol_version": aoe_protocol::VERSION,
        "asset_pack_version": 1,
        "rustc": "rustc 1.93.1 (fixture)",
        "e2e_report_hash": hash(&e2e).trim_start_matches("blake3:"),
        "perf_report_hash": hash(&perf).trim_start_matches("blake3:"),
        "sbom_hashes": sboms,
    });
    fs::write(
        release.join("published.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest");
    let mut checksums = String::new();
    for name in [
        "published.json",
        "bundle.tar",
        "server.spdx.json",
        "browser.spdx.json",
        "bundle.spdx.json",
        "e2e.json",
        "perf.json",
    ] {
        checksums.push_str(&format!(
            "{}  {name}\n",
            hash(&fs::read(release.join(name)).expect("file"))
        ));
    }
    fs::write(release.join("checksums.txt"), checksums).expect("checksums");
}

#[test]
fn release_cli_checks_git_tree_assets_and_registry_digests() {
    let workspace = tempfile::tempdir().expect("workspace");
    let root = workspace.path();
    command(root, &["init", "-q"]);
    fs::write(root.join("source.txt"), b"verified dev tree").expect("source file");
    command(root, &["add", "."]);
    command(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "source",
        ],
    );
    let revision = command(root, &["rev-parse", "HEAD"]);
    let tree = command(root, &["rev-parse", "HEAD^{tree}"]);
    command(root, &["branch", "-M", "main"]);
    command(root, &["update-ref", "refs/remotes/origin/dev", &revision]);
    fixture(root, &revision, &tree);

    let output_file = root.join("github-output");
    fs::write(&output_file, b"").expect("output file");
    let branch = format!("release/{revision}");
    let binary = env!("CARGO_BIN_EXE_aoe-harness");
    let source = Command::new(binary)
        .arg("release-source-check")
        .env("AOE_RELEASE_BRANCH", &branch)
        .env("GITHUB_OUTPUT", &output_file)
        .current_dir(root)
        .output()
        .expect("source check");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    assert!(
        fs::read_to_string(&output_file)
            .expect("source output")
            .contains(&format!("source_sha={revision}"))
    );
    let main_source = Command::new(binary)
        .arg("release-main-source-check")
        .env("GITHUB_OUTPUT", &output_file)
        .current_dir(root)
        .output()
        .expect("main source check");
    assert!(
        main_source.status.success(),
        "{}",
        String::from_utf8_lossy(&main_source.stderr)
    );

    let bin = root.join("fake-bin");
    fs::create_dir(&bin).expect("fake command directory");
    let docker = bin.join("docker");
    fs::write(&docker, b"#!/bin/sh\ncase \"$1\" in\n  login) cat >/dev/null ; exit 0 ;;\n  pull) exit 0 ;;\n  image) printf '[\"%s\"]' \"$5\" ; exit 0 ;;\nesac\nexit 1\n")
        .expect("fake Docker CLI");
    let mut permissions = fs::metadata(&docker).expect("Docker stub").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&docker, permissions).expect("executable Docker stub");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").expect("PATH"));
    let run = |subcommand: &str| {
        Command::new(binary)
            .arg(subcommand)
            .env("PATH", &path)
            .env("AOE_RELEASE_BRANCH", &branch)
            .env("AOE_RELEASE_SOURCE_SHA", &revision)
            .env("AOE_PREVIOUS_MANIFEST", root.join("missing-previous.json"))
            .env("GITHUB_REPOSITORY", "Anko59/AoeWorld")
            .env("GITHUB_TOKEN", "fixture-token")
            .env("GITHUB_ACTOR", "fixture")
            .env("GITHUB_OUTPUT", &output_file)
            .env("DOCKER_CONFIG", root.join("docker-config"))
            .current_dir(root)
            .output()
            .expect("release command")
    };
    let verification = run("release-verify-published");
    assert!(
        verification.status.success(),
        "{}",
        String::from_utf8_lossy(&verification.stderr)
    );
    assert!(
        fs::read_to_string(&output_file)
            .expect("digest outputs")
            .contains("server_image=ghcr.io/anko59/aoeworld-server@sha256:")
    );
    assert!(
        root.join("reports/release")
            .join(&revision)
            .join("verification.json")
            .is_file()
    );
    assert!(!run("release-rehearse-published").status.success());
    assert!(!run("release-smoke-published").status.success());
}
