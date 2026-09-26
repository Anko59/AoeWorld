use super::*;

#[test]
fn all_parser_targets_are_bounded_and_successfully_enforced() {
    let root = tempfile::tempdir().expect("directory");
    let storage_policy = storage::Policy::default();
    let storage_before = storage_policy.inspect(root.path()).expect("storage usage");
    for mode in [Mode::Smoke, Mode::Nightly] {
        let mut targets = Vec::new();
        let execution = execute(
            mode,
            root.path(),
            storage_policy,
            storage_before,
            |args, deadline| {
                targets.push(args[3].to_owned());
                assert_eq!(args[0], "+nightly-2026-09-01");
                assert_eq!(args[5], mode.limit());
                assert!(args.contains(&"-max_len=1048576"));
                assert_eq!(deadline, mode.deadline());
                Ok(())
            },
            |_, _, _| Ok(Vec::new()),
        );
        assert_eq!(targets, TARGETS);
        assert_eq!(execution.attempted_targets, TARGETS.len());
        assert_eq!(execution.successful_targets, TARGETS.len());
        assert_eq!(execution.target_results.len(), TARGETS.len());
        assert!(
            execution
                .target_results
                .iter()
                .all(|result| result.completed)
        );
        assert_eq!(execution.storage_after, storage_before);
        assert!(execution.failure.is_none());
    }
}

#[test]
fn quotas_are_enforced_after_each_target_without_deleting_inputs() {
    let root = tempfile::tempdir().expect("directory");
    let storage_before = storage::Snapshot {
        corpus: storage::Usage { files: 0, bytes: 0 },
        artifacts: storage::Usage { files: 0, bytes: 0 },
    };
    let storage_policy = storage::Policy {
        corpus_file_limit: 0,
        corpus_byte_limit: 0,
        artifact_file_limit: 0,
        artifact_byte_limit: 0,
        retention: storage::Policy::default().retention,
    };
    let mut calls = 0;
    let execution = execute(
        Mode::Nightly,
        root.path(),
        storage_policy,
        storage_before,
        |args, _| {
            calls += 1;
            assert_eq!(args[3], TARGETS[0]);
            let directory = root.path().join("fuzz/corpus/drs");
            fs::create_dir_all(&directory).expect("corpus directory");
            fs::write(directory.join("preserved-input"), b"keep me").expect("corpus input");
            Ok(())
        },
        |_, _, _| Ok(Vec::new()),
    );
    assert_eq!(calls, 1);
    assert_eq!(execution.attempted_targets, 1);
    assert_eq!(execution.successful_targets, 0);
    assert_eq!(execution.storage_after.corpus.files, 1);
    let failure = execution.failure.expect("quota failure");
    assert!(failure.contains("after target drs"));
    assert!(failure.contains("corpus files quota exceeded"));
    assert_eq!(
        fs::read(root.path().join("fuzz/corpus/drs/preserved-input")).unwrap(),
        b"keep me"
    );
}

#[test]
fn failure_reports_retain_the_after_snapshot_for_crash_inputs() {
    let root = tempfile::tempdir().expect("directory");
    let storage_before = storage::Snapshot {
        corpus: storage::Usage { files: 0, bytes: 0 },
        artifacts: storage::Usage { files: 0, bytes: 0 },
    };
    let execution = execute(
        Mode::Smoke,
        root.path(),
        storage::Policy::default(),
        storage_before,
        |args, _| {
            assert_eq!(args[3], TARGETS[0]);
            let directory = root.path().join("fuzz/artifacts/drs");
            fs::create_dir_all(&directory).expect("artifact directory");
            fs::write(directory.join("crash-input"), b"preserve crash").expect("artifact");
            Err("injected libFuzzer crash".into())
        },
        |_, _, _| Ok(Vec::new()),
    );
    assert_eq!(execution.attempted_targets, 1);
    assert_eq!(execution.successful_targets, 0);
    assert_eq!(execution.storage_after.artifacts.files, 1);
    assert_eq!(execution.storage_after.artifacts.bytes, 14);
    assert!(
        execution
            .failure
            .as_deref()
            .expect("failure accounting")
            .contains("target drs: injected libFuzzer crash")
    );

    write_report(
        root.path(),
        Report {
            version: 6,
            revision: "revision".into(),
            dirty: true,
            mode: Mode::Smoke.label(),
            toolchain: "nightly-2026-09-01",
            cargo_fuzz: "0.13.2",
            targets: TARGETS,
            limit: Mode::Smoke.limit(),
            prepared_seeds: Vec::new(),
            verified_legacy_seeds: Vec::new(),
            corpus_directory: "fuzz/corpus",
            artifact_directory: "fuzz/artifacts",
            storage_policy: storage::Policy::default(),
            storage_before,
            storage_after: execution.storage_after,
            maintenance_actions: Vec::new(),
            target_results: execution.target_results,
            attempted_targets: execution.attempted_targets,
            successful_targets: execution.successful_targets,
            result: "FAIL",
            failure: execution.failure,
        },
    )
    .expect("failure report");
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(root.path().join("reports/fuzz/smoke.json")).expect("report"),
    )
    .expect("JSON");
    assert_eq!(value["result"], "FAIL");
    assert_eq!(value["attempted_targets"], 1);
    assert_eq!(value["successful_targets"], 0);
    assert_ne!(value["storage_before"], value["storage_after"]);
    assert_eq!(value["storage_after"]["artifacts"]["files"], 1);
    assert!(
        value["failure"]
            .as_str()
            .expect("failure")
            .contains("injected libFuzzer crash")
    );
    assert_eq!(
        fs::read(root.path().join("fuzz/artifacts/drs/crash-input")).unwrap(),
        b"preserve crash"
    );
}

#[test]
fn failed_inter_target_maintenance_keeps_completed_target_distinct() {
    let root = tempfile::tempdir().expect("directory");
    let policy = storage::Policy::default();
    let before = policy.inspect(root.path()).expect("storage usage");
    let execution = execute(
        Mode::Nightly,
        root.path(),
        policy,
        before,
        |_, _| Ok(()),
        |target, _, _| Err(format!("injected maintenance failure after {target}").into()),
    );
    assert_eq!(execution.attempted_targets, 1);
    assert_eq!(execution.successful_targets, 1);
    assert!(execution.target_results[0].completed);
    assert!(
        execution
            .failure
            .as_deref()
            .is_some_and(|message| message.contains("corpus maintenance"))
    );
}

#[test]
fn reports_distinguish_bounded_smoke_and_nightly_campaigns() {
    let temp = tempfile::tempdir().expect("directory");
    const KNOWN_SEED_BYTES: &[u8] = b"";
    const KNOWN_SEED_BLAKE3_HEX: &str =
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";
    let seed = seeds::Seed::new(
        "map_chunk",
        "known-seed".into(),
        "fuzz/corpus/map_chunk/known-seed".into(),
        KNOWN_SEED_BYTES,
    );
    assert_eq!(seed.bytes, KNOWN_SEED_BYTES.len());
    assert_eq!(seed.blake3_hex, KNOWN_SEED_BLAKE3_HEX);
    let mut inventory = seeds::prepare(temp.path()).expect("seed inventory");
    inventory.prepared_seeds = vec![seed];
    let storage_policy = storage::Policy::default();
    let storage_before = storage::Snapshot {
        corpus: storage::Usage {
            files: 1,
            bytes: 10,
        },
        artifacts: storage::Usage { files: 0, bytes: 0 },
    };
    let storage_after = storage::Snapshot {
        corpus: storage::Usage {
            files: 2,
            bytes: 25,
        },
        artifacts: storage::Usage { files: 1, bytes: 8 },
    };
    for mode in [Mode::Smoke, Mode::Nightly] {
        write_report(
            temp.path(),
            Report {
                version: 6,
                revision: "revision".into(),
                dirty: true,
                mode: mode.label(),
                toolchain: "nightly-2026-09-01",
                cargo_fuzz: "0.13.2",
                targets: TARGETS,
                limit: mode.limit(),
                prepared_seeds: inventory.prepared_seeds.clone(),
                verified_legacy_seeds: inventory.verified_legacy_seeds.clone(),
                corpus_directory: "fuzz/corpus",
                artifact_directory: "fuzz/artifacts",
                storage_policy,
                storage_before,
                storage_after,
                maintenance_actions: Vec::new(),
                target_results: Vec::new(),
                attempted_targets: TARGETS.len(),
                successful_targets: TARGETS.len(),
                result: "PASS",
                failure: None,
            },
        )
        .expect("report");
        let path = temp
            .path()
            .join(format!("reports/fuzz/{}.json", mode.label()));
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("report")).expect("JSON");
        assert_eq!(value["result"], "PASS");
        assert_eq!(value["dirty"], true);
        assert_eq!(
            value["targets"].as_array().expect("targets").len(),
            TARGETS.len()
        );
        assert_eq!(value["version"], 6);
        assert_eq!(value["corpus_directory"], "fuzz/corpus");
        assert_eq!(value["artifact_directory"], "fuzz/artifacts");
        assert_eq!(
            value["prepared_seeds"][0]["blake3_hex"],
            KNOWN_SEED_BLAKE3_HEX
        );
        let legacy = value["verified_legacy_seeds"]
            .as_array()
            .expect("verified legacy seeds");
        assert_eq!(legacy.len(), 4);
        for (entry, expected) in legacy.iter().zip([
            (
                "fuzz/corpus/drs/one-entry.drs",
                "bdab473e663ab54eb10feb602b7cdc49360155889bdaebbcb50c113ad80308ef",
                include_bytes!("../../../../fuzz/corpus/drs/one-entry.drs").as_slice(),
            ),
            (
                "fuzz/corpus/manifest/minimal.json",
                "f904f26ecd0c86b2022e697bcad3ea3a3f4bd34c0494c06ba51d4123579b4400",
                include_bytes!("../../../../fuzz/corpus/manifest/minimal.json").as_slice(),
            ),
            (
                "fuzz/corpus/palette/jasc.pal",
                "bc31db7bdd8c5091d614bc0f8cb1396c4c05085c0ead9355953d1a4489ecfb5f",
                include_bytes!("../../../../fuzz/corpus/palette/jasc.pal").as_slice(),
            ),
            (
                "fuzz/corpus/slp/two-pixels.slp",
                "ab8a0a54eba7a0263293d72d6ac7862ae099e452bd88fcb06d934d9f33953ee3",
                include_bytes!("../../../../fuzz/corpus/slp/two-pixels.slp").as_slice(),
            ),
        ]) {
            let (path, digest, expected_bytes) = expected;
            assert_eq!(entry["path"], path);
            assert_eq!(entry["blake3_hex"], digest);
            let bytes = fs::read(temp.path().join(path)).expect("legacy bytes");
            assert_eq!(bytes, expected_bytes);
            assert_eq!(blake3::hash(&bytes).to_hex().to_string(), digest);
        }
        assert_eq!(
            value["storage_policy"]["retention"],
            "archive before coverage-guided active minimization; retain seeds and crashes"
        );
        assert_eq!(value["storage_before"]["corpus"]["files"], 1);
        assert_eq!(value["storage_before"]["corpus"]["bytes"], 10);
        assert_eq!(value["storage_after"]["corpus"]["files"], 2);
        assert_eq!(value["storage_after"]["corpus"]["bytes"], 25);
        assert_eq!(value["storage_after"]["artifacts"]["files"], 1);
        assert_ne!(value["storage_before"], value["storage_after"]);
        assert_eq!(value["attempted_targets"], TARGETS.len());
        assert_eq!(value["successful_targets"], TARGETS.len());
        assert!(value["failure"].is_null());
    }
}

#[test]
fn fuzz_report_revision_comes_from_git_and_rejects_unknown_refs() {
    assert_eq!(git(&["rev-parse", "HEAD"]).expect("revision").len(), 40);
    assert!(git(&["rev-parse", "this-ref-does-not-exist"]).is_err());
}

#[test]
fn live_quota_pressure_cancels_without_discarding_inputs() {
    let root = tempfile::tempdir().expect("directory");
    let policy = storage::Policy {
        corpus_file_limit: 10,
        corpus_byte_limit: 1_000,
        ..storage::Policy::default()
    };
    let directory = root.path().join("fuzz/corpus/drs");
    fs::create_dir_all(&directory).unwrap();
    for index in 0..10 {
        fs::write(directory.join(index.to_string()), b"preserved").unwrap();
    }
    let result = run_monitored(
        "sh",
        &["-c", "sleep 5"],
        Duration::from_secs(6),
        root.path(),
        policy,
    );
    assert!(result.unwrap_err().to_string().contains("95% quota guard"));
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 10);
}
