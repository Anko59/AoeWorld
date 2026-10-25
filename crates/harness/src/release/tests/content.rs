use super::*;

#[test]
fn bundle_hash_changes_on_tamper() {
    let directory = tempfile::tempdir().expect("tempdir");
    fs::write(directory.path().join("index.html"), "original").expect("write");
    let first = hash_bundle(directory.path()).expect("hash");
    fs::write(directory.path().join("index.html"), "tampered").expect("write");
    assert_ne!(first, hash_bundle(directory.path()).expect("hash"));
}

#[test]
fn bundle_inventory_rejects_empty_and_symlinked_content() {
    let empty = tempfile::tempdir().expect("directory");
    assert!(hash_bundle(empty.path()).is_err());
    let first = tempfile::tempdir().expect("first");
    let second = tempfile::tempdir().expect("second");
    for directory in [first.path(), second.path()] {
        fs::create_dir(directory.join("nested")).expect("nested");
    }
    fs::write(first.path().join("index.html"), b"home").expect("first file");
    fs::write(first.path().join("nested/app.js"), b"app").expect("nested file");
    fs::write(second.path().join("nested/app.js"), b"app").expect("nested file");
    fs::write(second.path().join("index.html"), b"home").expect("second file");
    assert_eq!(
        hash_bundle(first.path()).expect("first hash"),
        hash_bundle(second.path()).expect("second hash")
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("index.html", second.path().join("alias.html"))
            .expect("symlink");
        assert!(hash_bundle(second.path()).is_err());
    }
}

#[test]
fn release_evidence_rejects_wrong_revision_status_and_dirty_performance() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("evidence.json");
    fs::write(&path, br#"{"revision":"abc","result":"PASS"}"#).expect("E2E evidence");
    assert_eq!(
        evidence(&path, "abc", "result").expect("valid hash").len(),
        64
    );
    assert!(evidence(&path, "other", "result").is_err());
    fs::write(&path, br#"{"revision":"abc","result":"FAIL"}"#).expect("E2E fail");
    assert!(evidence(&path, "abc", "result").is_err());
    fs::write(
        &path,
        br#"{"revision":"abc","verdict":"PASS","dirty":true}"#,
    )
    .expect("dirty performance");
    assert!(evidence(&path, "abc", "verdict").is_err());
    fs::write(
        &path,
        br#"{"revision":"abc","verdict":"PASS","dirty":false}"#,
    )
    .expect("clean performance");
    assert!(evidence(&path, "abc", "verdict").is_ok());
}

#[test]
fn release_manifest_checks_format_tree_and_bundle_before_images() {
    let directory = tempfile::tempdir().expect("directory");
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).expect("bundle");
    fs::write(bundle.join("index.html"), b"original").expect("bundle file");
    let revision = output("git", &["rev-parse", "HEAD"]).expect("revision");
    let tree = output("git", &["rev-parse", "HEAD^{tree}"]).expect("tree");
    let mut manifest = Manifest {
        version: 2,
        source_commit: revision,
        source_tree: tree,
        server_image_id: "sha256:server".to_owned(),
        browser_image_id: "sha256:browser".to_owned(),
        bundle_hash: hash_bundle(&bundle).expect("bundle hash"),
        protocol_version: aoe_protocol::VERSION,
        asset_pack_version: 1,
        rustc: "test".to_owned(),
        e2e_report_hash: "a".repeat(64),
        perf_report_hash: "b".repeat(64),
    };
    let path = directory.path().join("manifest.json");
    let write = |manifest: &Manifest| {
        fs::write(&path, serde_json::to_vec(manifest).expect("JSON")).expect("manifest")
    };
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.version = 1;
    manifest.source_commit = "invalid".to_owned();
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.source_commit = output("git", &["rev-parse", "HEAD"]).expect("revision");
    manifest.source_tree = "wrong".to_owned();
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
    manifest.source_tree = output("git", &["rev-parse", "HEAD^{tree}"]).expect("tree");
    fs::write(bundle.join("index.html"), b"tampered").expect("tamper");
    write(&manifest);
    assert!(load_and_verify(&path).is_err());
}

#[test]
fn release_build_rejects_missing_runtime_shader_in_bundle_or_images() {
    for container in [
        "temporary-container",
        "server-container",
        "browser-container",
    ] {
        let temp = tempfile::tempdir().expect("checkout");
        release_evidence(temp.path());
        let runtime = FakeRuntime::default();
        *runtime.missing_shader_in.borrow_mut() = Some(container.into());
        let error = build_with(&runtime, temp.path()).expect_err("missing runtime shader rejected");
        assert!(
            error.to_string().contains("mixing.wgsl"),
            "{container}: {error}"
        );
        assert_no_release(temp.path());
        assert!(
            runtime
                .calls
                .borrow()
                .iter()
                .any(|args| args == &["docker", "rm", "-f", container])
        );
    }
}

#[test]
fn release_docker_and_shared_web_build_generate_external_shaders() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dockerfile = fs::read_to_string(root.join("docker/build-artifacts.Dockerfile")).unwrap();
    assert!(dockerfile.contains("cargo run --locked -p aoe-harness -- assets browser-shaders"));
    let commands = crate::ship::compare::build::web_commands("client.wasm", true, true);
    assert!(
        commands
            .iter()
            .any(|command| command.ends_with(&["assets", "browser-shaders"]))
    );
}

#[test]
fn generated_shader_inventory_matches_client_runtime_references() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let client = fs::read_to_string(root.join("crates/rendering/src/web/shaders.js")).unwrap();
    let names = client
        .split_once('[')
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split(',')
        .map(|name| name.trim().trim_matches('\''))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        crate::assets::browser_shaders()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    );
    assert!(client.contains("fetch(`/pkg/${name}.wgsl`)"));
    let web = tempfile::tempdir().unwrap();
    fs::create_dir(web.path().join("pkg")).unwrap();
    for (name, shader) in crate::assets::browser_shaders() {
        fs::write(web.path().join(format!("pkg/{name}.wgsl")), shader).unwrap();
    }
    content::verify_shaders(web.path()).unwrap();
    fs::write(web.path().join("pkg/ordinary.wgsl"), "stale shader").unwrap();
    assert!(
        content::verify_shaders(web.path())
            .unwrap_err()
            .to_string()
            .contains("ordinary.wgsl")
    );
}

#[test]
fn release_rejects_bundle_inventory_changes_during_image_extraction() {
    let temp = tempfile::tempdir().unwrap();
    release_evidence(temp.path());
    let runtime = FakeRuntime::default();
    runtime.remove_bundle_file_on_image_copy.set(true);
    let error = build_with(&runtime, temp.path()).expect_err("bundle inventory changed");
    assert!(error.to_string().contains("bundle changed"), "{error}");
    assert_no_release(temp.path());
}
