use super::*;

#[test]
fn the_newest_aoeworld_wheel_is_chosen_and_other_files_are_ignored() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        newest_wheel(directory.path()).is_err(),
        "no wheel is an error"
    );
    let old = directory
        .path()
        .join("aoeworld-0.0.9-cp39-abi3-linux_x86_64.whl");
    std::fs::write(&old, b"old").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let new = directory
        .path()
        .join("aoeworld-0.1.0-cp39-abi3-linux_x86_64.whl");
    std::fs::write(&new, b"new").unwrap();
    std::fs::write(directory.path().join("other-1.0.whl"), b"x").unwrap();
    std::fs::write(directory.path().join("aoeworld-0.2.0.tar.gz"), b"x").unwrap();
    assert_eq!(newest_wheel(directory.path()).unwrap(), new);
}

#[test]
fn bindings_paths_match_the_repository_layout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = vec![MANIFEST, AI_SOURCE];
    for (tests, project) in TESTS {
        paths.extend([tests, project]);
    }
    for path in paths {
        assert!(root.join(path).exists(), "{path} is missing");
    }
}

#[test]
fn every_ai_test_without_a_server_runs_in_the_logic_suite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tests = logic_tests(&root).unwrap();
    // Never listed by hand before discovery, so it had never run.
    assert!(
        tests.contains(&"ai/tests/test_bridges.py".to_owned()),
        "{tests:?}"
    );
    assert!(
        tests.contains(&"ai/tests/scaling/test_kingdom.py".to_owned()),
        "{tests:?}"
    );
    assert!(
        !tests
            .iter()
            .any(|test| SERVER_TESTS.contains(&test.as_str()))
    );
    assert!(tests.iter().all(|test| test.ends_with(".py")));
}

#[test]
fn a_stale_server_exclusion_fails_loudly() {
    let empty = tempfile::tempdir().unwrap();
    let error = logic_tests(empty.path()).unwrap_err().to_string();
    assert!(error.contains("does not exist"), "{error}");
}

#[test]
fn benchmark_requires_a_real_package_and_has_bounded_player_and_time_arguments() {
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        action: Action,
    }
    assert!(Cli::try_parse_from(["sdk", "benchmark"]).is_err());
    assert!(
        Cli::try_parse_from([
            "sdk",
            "benchmark",
            "--map-hash",
            &"a".repeat(64),
            "--bots",
            "0"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "sdk",
            "benchmark",
            "--map-hash",
            &"a".repeat(64),
            "--bots",
            "101"
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["sdk", "benchmark", "--map-hash", "bad"]).is_err());
    assert!(
        Cli::try_parse_from([
            "sdk",
            "benchmark",
            "--map-hash",
            &"a".repeat(64),
            "--out",
            ".cache/ai-scaling/baseline.json"
        ])
        .is_ok()
    );
}

#[test]
fn bindings_can_be_tested_without_launching_long_ai_games() {
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        action: Action,
    }
    let parsed = Cli::try_parse_from(["sdk", "test", "--bindings-only"]).unwrap();
    assert!(matches!(
        parsed.action,
        Action::Test {
            bindings_only: true,
            ..
        }
    ));
    let parsed =
        Cli::try_parse_from(["sdk", "test", "--only", "ai/tests/a.py", "ai/tests/b.py"]).unwrap();
    assert!(matches!(parsed.action, Action::Test { only, .. } if only.len() == 2));
}

#[test]
fn benchmark_deadline_covers_the_documented_120_minute_run_and_is_configurable() {
    use clap::Parser;
    #[derive(clap::Parser)]
    struct Cli {
        #[command(subcommand)]
        action: Action,
    }
    let parse = |extra: &[&str]| {
        let mut argv = vec![
            "sdk",
            "benchmark",
            "--map-hash",
            &"a".repeat(64),
            "--out",
            "x.json",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        argv.extend(extra.iter().map(|value| value.to_string()));
        match Cli::try_parse_from(argv).unwrap().action {
            Action::Benchmark(args) => args,
            _ => unreachable!(),
        }
    };
    let default = benchmark::wall_deadline(&parse(&[]));
    assert!(default >= Duration::from_secs(60 * 60 * 4), "{default:?}");
    let short = benchmark::wall_deadline(&parse(&["--wall-minutes", "30"]));
    assert_eq!(short, Duration::from_secs(60 * 30));
    assert!(
        Cli::try_parse_from([
            "sdk",
            "benchmark",
            "--map-hash",
            &"a".repeat(64),
            "--wall-minutes",
            "0"
        ])
        .is_err()
    );
}

fn scratch_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ai/tests")).unwrap();
    std::fs::write(dir.path().join("ai/tests/test_start_kit.py"), "").unwrap();
    dir
}

#[test]
fn only_normalises_spellings_to_the_same_selection() {
    let repo = scratch_repo();
    let root = repo.path();
    let wanted = vec![vec![], vec!["ai/tests/test_start_kit.py".to_string()]];
    let absolute = format!("{}/ai/tests/test_start_kit.py", root.display());
    for spelling in [
        "ai/tests/test_start_kit.py",
        "./ai/tests/test_start_kit.py",
        "ai/tests/../tests/test_start_kit.py",
        absolute.as_str(),
    ] {
        let got = select_targets(root, &TESTS, &[spelling.to_string()]).unwrap();
        assert_eq!(got, wanted, "{spelling}");
    }
    let node = select_targets(
        root,
        &TESTS,
        &["./ai/tests/test_start_kit.py::test_a".into()],
    );
    assert_eq!(node.unwrap()[1], ["ai/tests/test_start_kit.py::test_a"]);
}

#[test]
fn only_target_matching_nothing_is_an_error_naming_the_directories() {
    let repo = scratch_repo();
    for target in [
        "docs/readme.md",
        "../ai/tests/test_start_kit.py",
        "ai/tests_other/x.py",
        "ai/tests/missing_file.py",
    ] {
        let error = select_targets(repo.path(), &TESTS, &[target.to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains(target), "{error}");
    }
    let error = select_targets(repo.path(), &TESTS, &["docs/a.py".into()])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("crates/sdk-python/tests") && error.contains("ai/tests"),
        "{error}"
    );
    // With --bindings-only the AI directory is not searched, so it is a miss too.
    assert!(
        select_targets(
            repo.path(),
            &TESTS[..1],
            &["ai/tests/test_start_kit.py".into()]
        )
        .is_err()
    );
}
