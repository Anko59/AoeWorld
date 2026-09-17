use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct FakeRuntime {
    calls: RefCell<Vec<Vec<String>>>,
    reject_ancestor: Cell<bool>,
    reject_digest: Cell<bool>,
    main_is_bootstrap: Cell<bool>,
    tree_mismatch: Cell<bool>,
}

impl Runtime for FakeRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        self.calls.borrow_mut().push(
            std::iter::once(program.to_owned())
                .chain(args.iter().map(|item| (*item).to_owned()))
                .collect(),
        );
        if program == "git" {
            if args.last().is_some_and(|arg| *arg == "origin/main") {
                return Ok(if self.main_is_bootstrap.get() {
                    BOOTSTRAP_MAIN.to_owned()
                } else {
                    "f".repeat(40)
                });
            }
            if self.tree_mismatch.get() && args.last().is_some_and(|arg| *arg == "HEAD^{tree}") {
                return Ok("b".repeat(40));
            }
            return Ok("a".repeat(40));
        }
        let reference = args.last().ok_or("image reference missing")?;
        let items = if self.reject_digest.get() {
            vec!["ghcr.io/other@sha256:wrong".to_owned()]
        } else {
            vec![(*reference).to_owned()]
        };
        Ok(serde_json::to_string(&items)?)
    }

    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        self.calls.borrow_mut().push(
            std::iter::once(program.to_owned())
                .chain(args.iter().map(|item| (*item).to_owned()))
                .collect(),
        );
        if program == "git" && self.reject_ancestor.get() {
            return Err("source is not on dev".into());
        }
        Ok(())
    }
}

fn file_hash(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn fixture(directory: &Path, source: &str, tree: &str, registry: &str) -> PublishedManifest {
    let e2e = serde_json::json!({"revision": source, "result": "PASS"});
    let perf = serde_json::json!({"revision": source, "dirty": false, "verdict": "PASS"});
    let e2e_bytes = serde_json::to_vec(&e2e).expect("E2E JSON");
    let perf_bytes = serde_json::to_vec(&perf).expect("performance JSON");
    fs::write(directory.join("e2e.json"), &e2e_bytes).expect("E2E file");
    fs::write(directory.join("perf.json"), &perf_bytes).expect("performance file");
    fs::write(directory.join("bundle.tar"), b"fixture archive").expect("bundle archive");
    for name in ["server.spdx.json", "browser.spdx.json", "bundle.spdx.json"] {
        fs::write(directory.join(name), b"{\"spdxVersion\":\"SPDX-2.3\"}").expect("SBOM");
    }
    let published = PublishedManifest {
        version: 1,
        source_commit: source.to_owned(),
        source_tree: tree.to_owned(),
        server_image: format!("{registry}-server@sha256:{}", "b".repeat(64)),
        browser_image: format!("{registry}-browser@sha256:{}", "c".repeat(64)),
        bundle_hash: "bound by local release".into(),
        bundle_archive_hash: file_hash(b"fixture archive"),
        protocol_version: aoe_protocol::VERSION,
        asset_pack_version: 1,
        rustc: "rustc 1.93.1 (fixture)".into(),
        e2e_report_hash: file_hash(&e2e_bytes).trim_start_matches("blake3:").into(),
        perf_report_hash: file_hash(&perf_bytes).trim_start_matches("blake3:").into(),
        sbom_hashes: release_publish::sbom_hashes(directory).expect("SBOM hashes"),
    };
    fs::write(
        directory.join("published.json"),
        serde_json::to_vec_pretty(&published).expect("published JSON"),
    )
    .expect("published manifest");
    let names = [
        "published.json",
        "bundle.tar",
        "server.spdx.json",
        "browser.spdx.json",
        "bundle.spdx.json",
        "e2e.json",
        "perf.json",
    ];
    let mut checksums = String::new();
    for name in names {
        let bytes = fs::read(directory.join(name)).expect("release file");
        checksums.push_str(&format!("{}  {name}\n", file_hash(&bytes)));
    }
    fs::write(directory.join("checksums.txt"), checksums).expect("checksums");
    published
}

#[test]
fn source_branch_requires_the_exact_verified_dev_tree() {
    let runtime = FakeRuntime::default();
    let source = "d".repeat(40);
    assert_eq!(
        source_with(&runtime, &format!("release/{source}")).expect("source"),
        (source.clone(), "a".repeat(40))
    );
    assert!(source_with(&runtime, "release/short").is_err());
    assert!(source_with(&runtime, &format!("feature/{source}")).is_err());
    runtime.reject_ancestor.set(true);
    assert!(source_with(&runtime, &format!("release/{source}")).is_err());
    runtime.reject_ancestor.set(false);
    runtime.tree_mismatch.set(true);
    assert!(source_with(&runtime, &format!("release/{source}")).is_err());
}

#[test]
fn release_pr_flow_reports_exact_candidate_and_rollback_references() {
    let workspace = tempfile::tempdir().expect("workspace");
    let root = workspace.path().join("release");
    let source = "d".repeat(40);
    let previous_source = "e".repeat(40);
    let registry = "ghcr.io/anko59/aoeworld";
    let candidate_dir = root.join(&source);
    let previous_dir = root.join("previous");
    fs::create_dir_all(&candidate_dir).expect("candidate directory");
    fs::create_dir_all(&previous_dir).expect("previous directory");
    let candidate = fixture(&candidate_dir, &source, &"a".repeat(40), registry);
    let previous = fixture(&previous_dir, &previous_source, &"a".repeat(40), registry);
    let branch = format!("release/{source}");
    let runtime = FakeRuntime::default();
    let github_output = workspace.path().join("github-output");
    fs::write(&github_output, b"").expect("output file");
    verify_with(
        &runtime,
        &branch,
        &source,
        registry,
        &root,
        Some(&github_output),
    )
    .expect("published verification");
    assert!(
        fs::read_to_string(&github_output)
            .expect("workflow outputs")
            .contains(&candidate.server_image)
    );
    assert!(
        fs::read_to_string(candidate_dir.join("verification.json"))
            .expect("verification report")
            .contains("\"verdict\": \"PASS\"")
    );
    assert!(verify_with(&runtime, &branch, "wrong", registry, &root, None).is_err());

    rehearse_with(
        &runtime,
        &branch,
        registry,
        &root,
        &previous_dir.join("published.json"),
        |candidate_ref, previous_ref| {
            assert_eq!(candidate_ref.server_image, candidate.server_image);
            assert_eq!(previous_ref.browser_image, previous.browser_image);
            Ok(())
        },
    )
    .expect("promotion rehearsal");
    assert!(
        fs::read_to_string(candidate_dir.join("promotion.json"))
            .expect("promotion report")
            .contains("\"verdict\": \"PASS\"")
    );

    runtime.main_is_bootstrap.set(true);
    smoke_with(&runtime, &branch, registry, &root, |candidate_ref| {
        assert_eq!(candidate_ref.browser_image, candidate.browser_image);
        Ok(())
    })
    .expect("first release smoke");
    assert!(
        fs::read_to_string(candidate_dir.join("promotion.json"))
            .expect("bootstrap report")
            .contains("\"rollback_rehearsed\": false")
    );
    runtime.main_is_bootstrap.set(false);
    assert!(smoke_with(&runtime, &branch, registry, &root, |_| Ok(())).is_err());
}

#[test]
fn runtime_surfaces_failed_commands() {
    let runtime = RealRuntime;
    assert_eq!(
        runtime.output("printf", &["release"]).expect("output"),
        "release"
    );
    assert!(runtime.checked("true", &[]).is_ok());
    assert!(runtime.checked("false", &[]).is_err());
    assert!(runtime.output("false", &[]).is_err());
}

struct MainRuntime {
    branch: &'static str,
    duplicate_tree: bool,
}

impl Runtime for MainRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        if program != "git" {
            return Err("unexpected program".into());
        }
        match args {
            ["branch", "--show-current"] => Ok(self.branch.into()),
            ["log", "origin/dev", "--format=%H"] => {
                Ok(format!("{}\n{}", "b".repeat(40), "c".repeat(40)))
            }
            ["rev-parse", "HEAD^{tree}"] => Ok("a".repeat(40)),
            ["rev-parse", revision] if revision.starts_with(&"b".repeat(40)) => Ok("a".repeat(40)),
            ["rev-parse", revision] if revision.starts_with(&"c".repeat(40)) => {
                Ok(if self.duplicate_tree { "a" } else { "d" }.repeat(40))
            }
            _ => Err(format!("unexpected Git query: {args:?}").into()),
        }
    }

    fn checked(&self, _: &str, _: &[&str]) -> Result<()> {
        Ok(())
    }
}

#[test]
fn main_promotion_requires_a_unique_matching_dev_tree() {
    assert_eq!(
        main_source_with(&MainRuntime {
            branch: "main",
            duplicate_tree: false,
        })
        .expect("source"),
        "b".repeat(40)
    );
    assert!(
        main_source_with(&MainRuntime {
            branch: "feature",
            duplicate_tree: false,
        })
        .is_err()
    );
    assert!(
        main_source_with(&MainRuntime {
            branch: "main",
            duplicate_tree: true,
        })
        .is_err()
    );
}

#[test]
fn published_evidence_checksums_and_image_digests_are_exact() {
    let directory = tempfile::tempdir().expect("directory");
    let source = "d".repeat(40);
    let tree = "a".repeat(40);
    let registry = "ghcr.io/anko59/aoeworld";
    let published = fixture(directory.path(), &source, &tree, registry);
    let verified = verify_files(directory.path(), &source, &tree, registry).expect("files");
    assert_eq!(verified.server_image, published.server_image);
    let runtime = FakeRuntime::default();
    pull_and_verify(&runtime, &verified).expect("image digests");
    assert_eq!(
        runtime
            .calls
            .borrow()
            .iter()
            .filter(|call| call.get(1).is_some_and(|arg| arg == "pull"))
            .count(),
        2
    );
    runtime.reject_digest.set(true);
    assert!(pull_and_verify(&runtime, &verified).is_err());
    report(
        &directory.path().join("verification.json"),
        &verified,
        "PASS",
    )
    .expect("verification report");
    assert!(
        fs::read_to_string(directory.path().join("verification.json"))
            .expect("report")
            .contains("\"verdict\": \"PASS\"")
    );

    assert!(verify_files(directory.path(), "wrong", &tree, registry).is_err());
    assert!(verify_files(directory.path(), &source, "wrong", registry).is_err());
    assert!(verify_files(directory.path(), &source, &tree, "ghcr.io/other/repo").is_err());
    fs::write(directory.path().join("bundle.tar"), b"tampered").expect("tamper");
    assert!(verify_files(directory.path(), &source, &tree, registry).is_err());
}
