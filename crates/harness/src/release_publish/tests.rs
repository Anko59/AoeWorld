use super::*;
use std::cell::RefCell;

#[derive(Default)]
struct FakeRuntime {
    calls: RefCell<Vec<Vec<String>>>,
}

impl Runtime for FakeRuntime {
    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        self.calls.borrow_mut().push(
            std::iter::once(program.to_owned())
                .chain(args.iter().map(|item| (*item).to_owned()))
                .collect(),
        );
        if program == "tar" {
            fs::write(args[3], b"synthetic archive")?;
        }
        Ok(())
    }

    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        self.calls.borrow_mut().push(
            std::iter::once(program.to_owned())
                .chain(args.iter().map(|item| (*item).to_owned()))
                .collect(),
        );
        let tag = args.last().ok_or("tag missing")?;
        let name = tag.split(':').next().ok_or("name missing")?;
        Ok(serde_json::to_string(&vec![format!(
            "{name}@sha256:{}",
            "a".repeat(64)
        )])?)
    }
}

fn local_manifest() -> Manifest {
    Manifest {
        version: 1,
        source_commit: "b".repeat(40),
        source_tree: "c".repeat(40),
        server_image_id: format!("sha256:{}", "d".repeat(64)),
        browser_image_id: format!("sha256:{}", "e".repeat(64)),
        bundle_hash: "bundle-hash".into(),
        protocol_version: aoe_protocol::VERSION,
        asset_pack_version: 1,
        rustc: "rustc 1.93.1".into(),
        e2e_report_hash: "e2e-hash".into(),
        perf_report_hash: "perf-hash".into(),
    }
}

#[test]
fn registry_and_digest_inputs_are_strict() {
    assert_eq!(
        valid_registry("Anko59/AoeWorld").expect("registry"),
        "ghcr.io/anko59/aoeworld"
    );
    for invalid in [
        "",
        "owner",
        "owner/repo/extra",
        "owner/repo_name",
        "owner/../repo",
    ] {
        assert!(valid_registry(invalid).is_err(), "accepted {invalid}");
    }
    let name = "ghcr.io/anko59/aoeworld-server";
    let reference = format!("{name}@sha256:{}", "a".repeat(64));
    assert_eq!(
        repo_digest(
            &serde_json::to_string(&vec![reference.clone()]).expect("JSON"),
            name
        )
        .expect("digest"),
        reference
    );
    assert!(repo_digest("[]", name).is_err());
    assert!(repo_digest("[\"ghcr.io/other@sha256:00\"]", name).is_err());
}

#[test]
fn published_manifest_binds_exact_digest_bundle_and_sboms() {
    let directory = tempfile::tempdir().expect("release directory");
    fs::create_dir(directory.path().join("bundle")).expect("bundle");
    fs::write(directory.path().join("bundle/index.html"), b"fixture").expect("bundle file");
    for name in ["server.spdx.json", "browser.spdx.json", "bundle.spdx.json"] {
        fs::write(
            directory.path().join(name),
            b"{\"spdxVersion\":\"SPDX-2.3\",\"packages\":[]}",
        )
        .expect("SBOM");
    }
    let runtime = FakeRuntime::default();
    let local = local_manifest();
    let published = push_with(
        &runtime,
        &local,
        directory.path(),
        "ghcr.io/anko59/aoeworld",
    )
    .expect("published manifest");
    assert_eq!(published.source_commit, local.source_commit);
    assert_eq!(published.bundle_hash, local.bundle_hash);
    assert!(
        published
            .server_image
            .starts_with("ghcr.io/anko59/aoeworld-server@sha256:")
    );
    assert!(
        published
            .browser_image
            .starts_with("ghcr.io/anko59/aoeworld-browser@sha256:")
    );
    assert_eq!(published.sbom_hashes.len(), 3);
    assert!(published.bundle_archive_hash.starts_with("blake3:"));
    assert_eq!(
        runtime
            .calls
            .borrow()
            .iter()
            .filter(|args| args.get(1).is_some_and(|arg| arg == "push"))
            .count(),
        2
    );
    fs::write(directory.path().join("e2e.json"), b"e2e evidence").expect("E2E");
    fs::write(directory.path().join("perf.json"), b"performance evidence").expect("performance");
    let github_output = directory.path().join("github-output");
    fs::write(&github_output, b"").expect("output file");
    finalize(directory.path(), &published, Some(&github_output)).expect("publication files");
    let stored: PublishedManifest = serde_json::from_slice(
        &fs::read(directory.path().join("published.json")).expect("published manifest"),
    )
    .expect("published JSON");
    assert_eq!(stored.server_image, published.server_image);
    assert_eq!(
        fs::read_to_string(directory.path().join("checksums.txt"))
            .expect("checksums")
            .lines()
            .count(),
        7
    );
    let outputs = fs::read_to_string(github_output).expect("outputs");
    assert!(outputs.contains("server_digest=sha256:"));
    assert!(outputs.contains("browser_name=ghcr.io/anko59/aoeworld-browser"));
    assert!(validate_context("refs/heads/dev", &local.source_commit, &local).is_ok());
    assert!(validate_context("refs/heads/main", &local.source_commit, &local).is_err());
    assert!(validate_context("refs/heads/dev", "wrong", &local).is_err());
    fs::write(directory.path().join("bundle.spdx.json"), b"{}").expect("tamper");
    assert!(
        push_with(
            &runtime,
            &local,
            directory.path(),
            "ghcr.io/anko59/aoeworld"
        )
        .is_err()
    );
    fs::remove_file(directory.path().join("perf.json")).expect("remove evidence");
    assert!(finalize(directory.path(), &published, None).is_err());
}

#[test]
fn process_runtime_propagates_child_failures() {
    let runtime = RealRuntime;
    assert_eq!(
        runtime.output("printf", &["release"]).expect("stdout"),
        "release"
    );
    assert!(runtime.checked("true", &[]).is_ok());
    assert!(runtime.checked("false", &[]).is_err());
    assert!(runtime.output("false", &[]).is_err());
}
