use super::*;
use crate::perf::Verdict;

fn line(name: &str, instructions: u64) -> String {
    serde_json::json!({
        "version": "6",
        "module_path": name,
        "profiles": [
            {
                "tool": "Callgrind",
                "summaries": {"total": {"summary": {
                    "Callgrind": {"Ir": {"metrics": {"Both": [{"Int": instructions}]}}}
                }}}
            }
        ]
    })
    .to_string()
}

#[test]
fn missing_samples_cannot_pass() {
    assert!(sample("{\"version\":\"6\",\"module_path\":\"x\"}").is_err());
    assert!(
        sample(&line("kernel", 100).replace("\"version\":\"6\"", "\"version\":\"5\"")).is_err()
    );
    let (name, instructions) = sample(&line("kernel", 100)).expect("sample");
    assert_eq!((name.as_str(), instructions), ("kernel", 100));

    let temp = tempfile::NamedTempFile::new().expect("file");
    assert!(samples(temp.path()).is_err());
    fs::write(
        temp.path(),
        format!("{}\n{}\n", line("same", 100), line("same", 100)),
    )
    .expect("samples");
    assert!(samples(temp.path()).is_err());
}

#[test]
fn baseline_identity_case_set_and_regressions_are_checked() {
    let mut observed = BTreeMap::new();
    for name in ["a", "b", "c", "d", "e", "f", "g"] {
        observed.insert(name.to_owned(), 100);
    }
    assert!(proposal(&BTreeMap::new(), &BTreeMap::new()).is_err());
    let baseline = proposal(&observed, &BTreeMap::new()).expect("proposal");
    assert!(compare_observed(baseline, BTreeMap::new()).is_err());

    let mut baseline = proposal(&observed, &BTreeMap::new()).expect("proposal");
    baseline.toolchain = "different".to_owned();
    assert!(compare_observed(baseline, observed.clone()).is_err());

    let baseline = proposal(&observed, &BTreeMap::new()).expect("proposal");
    observed.insert("a".to_owned(), 104);
    let results = compare_observed(baseline, observed).expect("comparison");
    assert_eq!(results.len(), 7);
    assert_eq!(results[0].verdict, Verdict::Regression);
    assert!(
        results[1..]
            .iter()
            .all(|item| item.verdict == Verdict::Pass)
    );
}

#[test]
fn proposal_reads_both_suite_files_without_changing_baseline() {
    let temp = tempfile::tempdir().expect("directory");
    let simulation = temp.path().join("simulation.ndjson");
    let protocol = temp.path().join("protocol.ndjson");
    let assets = temp.path().join("assets.ndjson");
    let baseline = temp.path().join("baseline.json");
    let proposed = temp.path().join("proposal/micro.json");
    fs::write(
        &simulation,
        format!(
            "{}\n{}\n{}\n{}\n{}\n",
            line("a", 100),
            line("b", 100),
            line("e", 100),
            line("f", 100),
            line("g", 100)
        ),
    )
    .expect("simulation samples");
    fs::write(&protocol, format!("{}\n", line("c", 100))).expect("protocol samples");
    fs::write(&assets, format!("{}\n", line("d", 100))).expect("asset samples");
    assert_eq!(
        observed_samples_at(&simulation, &protocol, &assets)
            .expect("samples")
            .len(),
        7
    );
    let original = proposal(
        &observed_samples_at(&simulation, &protocol, &assets).expect("samples"),
        &BTreeMap::new(),
    )
    .expect("baseline");
    let original_bytes = serde_json::to_vec(&original).expect("JSON");
    fs::write(&baseline, &original_bytes).expect("baseline");
    propose_at(&simulation, &protocol, &assets, &baseline, &proposed).expect("proposal");
    assert_eq!(fs::read(&baseline).expect("baseline"), original_bytes);
    assert_eq!(
        comparisons_at(
            &baseline,
            &simulation,
            &protocol,
            &assets,
            &temp.path().join("absent.json")
        )
        .expect("comparison")
        .0
        .len(),
        7
    );
    assert!(proposed.is_file());
    fs::write(&protocol, format!("{}\n", line("a", 100))).expect("duplicate");
    assert!(observed_samples_at(&simulation, &protocol, &assets).is_err());
}

#[test]
fn late_game_identity_and_complete_case_set_are_required() {
    let observed: BTreeMap<_, _> = ["a", "b", "c", "d", "e", "f", "g"]
        .into_iter()
        .map(|name| (name.to_owned(), 100))
        .collect();
    let mut baseline = proposal(&observed, &BTreeMap::new()).unwrap();
    baseline.late_game_workload_hash = "different workload".into();
    assert!(compare_observed(baseline, observed.clone()).is_err());
    let mut baseline = proposal(&observed, &BTreeMap::new()).unwrap();
    baseline.instructions.remove("e");
    baseline.instruction_reference.remove("e");
    let mut partial = observed;
    partial.remove("e");
    assert!(compare_observed(baseline, partial).is_err());
}

fn judged(observed: u64, baseline: u64, reference: u64) -> (Verdict, Option<String>) {
    judged_with(observed, baseline, reference, None)
}

fn judged_with(
    observed: u64,
    baseline: u64,
    reference: u64,
    latest: Option<(u64, u64)>,
) -> (Verdict, Option<String>) {
    let row = judge("instructions::k", observed, baseline, reference, latest);
    (row.verdict, row.note)
}

#[test]
fn instruction_rows_pass_warn_fail_and_ask_for_refresh_at_the_agreed_thresholds() {
    let (verdict, note) = judged(1_000, 1_000, 1_000);
    assert_eq!((verdict, note), (Verdict::Pass, None));
    // +1% exactly is quiet; one instruction more warns.
    assert_eq!(judged(1_010, 1_000, 1_000), (Verdict::Pass, None));
    let (verdict, note) = judged(1_011, 1_000, 1_000);
    assert_eq!(verdict, Verdict::Pass);
    assert!(note.expect("warning").starts_with("warning"));
    // +3% exactly passes with a warning; one more fails.
    let (verdict, note) = judged(1_030, 1_000, 1_000);
    assert_eq!(verdict, Verdict::Pass);
    assert!(note.expect("warning").starts_with("warning"));
    let (verdict, note) = judged(1_031, 1_000, 1_000);
    assert_eq!(verdict, Verdict::Regression);
    assert!(note.expect("failure").starts_with("FAIL"));
    // -3% exactly is quiet; below it asks for a refresh.
    assert_eq!(judged(970, 1_000, 1_000), (Verdict::Pass, None));
    let (verdict, note) = judged(969, 1_000, 1_000);
    assert_eq!(verdict, Verdict::Pass);
    assert!(note.expect("refresh").contains("refresh the baseline"));
}

#[test]
fn creep_past_the_fixed_reference_fails_even_when_every_step_was_small() {
    // The baseline was refreshed upward in steps; 2% over it is only a warning
    // until the reference is more than 10% behind.
    assert_eq!(judged(1_100, 1_100, 1_000), (Verdict::Pass, None));
    let (verdict, note) = judged(1_122, 1_100, 1_000);
    assert_eq!(verdict, Verdict::Regression);
    assert!(note.expect("creep").contains("creep"));
    let (verdict, note) = judged(1_100, 1_100, 990);
    assert_eq!(verdict, Verdict::Regression);
    assert!(note.expect("creep").contains("reference 990"));
    // An improvement over the reference never trips the guard.
    assert_eq!(judged(500, 500, 1_000), (Verdict::Pass, None));
}

#[test]
fn proposal_keeps_the_committed_reference_and_starts_new_cases_at_observed() {
    let observed: BTreeMap<_, _> = [("a".to_owned(), 90), ("b".to_owned(), 50)].into();
    let reference: BTreeMap<_, _> = [("a".to_owned(), 100)].into();
    // Seven cases are required; the two-case map is rejected, so check the
    // anchoring through the same helper with the full set.
    assert!(proposal(&observed, &reference).is_err());
    let mut full: BTreeMap<_, _> = ["a", "b", "c", "d", "e", "f", "g"]
        .into_iter()
        .map(|name| (name.to_owned(), 50))
        .collect();
    full.insert("a".to_owned(), 90);
    let proposed = proposal(&full, &reference).expect("proposal");
    assert_eq!(proposed.instructions["a"], 90);
    assert_eq!(proposed.instruction_reference["a"], 100);
    assert_eq!(proposed.instruction_reference["b"], 50);
}

fn dev_reference(late: u64, workloads: BTreeMap<String, String>) -> dev::DevReference {
    dev::DevReference {
        run: 42,
        revision: "abc".to_owned(),
        workloads,
        instructions: [("simulation::late_game".to_owned(), late)].into(),
    }
}

#[test]
fn regression_over_the_latest_dev_run_fails_even_under_a_stale_baseline() {
    // Baseline is stale at 1_000; dev improved to 800. 880 is +10% over dev but
    // only -12% vs the baseline: the old gate passed it, this one fails it.
    let (verdict, note) = judged_with(880, 1_000, 1_000, Some((800, 42)));
    assert_eq!(verdict, Verdict::Regression);
    let note = note.expect("failure");
    assert!(
        note.starts_with("FAIL") && note.contains("dev run 42"),
        "{note}"
    );
    // +3% exactly over dev passes; one instruction more fails.
    assert_eq!(
        judged_with(824, 1_000, 1_000, Some((800, 42))).0,
        Verdict::Pass
    );
    assert_eq!(
        judged_with(825, 1_000, 1_000, Some((800, 42))).0,
        Verdict::Regression
    );
}

#[test]
fn improvement_over_dev_passes_and_the_baseline_checks_stay() {
    assert_eq!(
        judged_with(700, 1_000, 1_000, Some((800, 42))).0,
        Verdict::Pass
    );
    assert_eq!(
        judged_with(800, 800, 800, Some((800, 42))),
        (Verdict::Pass, None)
    );
    // Dev being generous never loosens the committed baseline.
    assert_eq!(
        judged_with(1_031, 1_000, 1_000, Some((2_000, 42))).0,
        Verdict::Regression
    );
}

#[test]
fn dev_counts_gate_each_kernel_and_unlisted_kernels_use_the_baseline_only() {
    let observed: BTreeMap<_, _> = [
        "instructions::simulation::late_game",
        "instructions::b",
        "instructions::c",
        "instructions::d",
        "instructions::e",
        "instructions::f",
        "instructions::g",
    ]
    .into_iter()
    .map(|name| (name.to_owned(), 100))
    .collect();
    let mut baseline = proposal(&observed, &BTreeMap::new()).unwrap();
    for value in baseline.instructions.values_mut() {
        *value = 130;
    }
    let dev = dev_reference(90, dev::identity());
    let rows = compare_with_dev(baseline, observed, Some(&dev)).unwrap();
    let late = rows
        .iter()
        .find(|row| row.metric.ends_with("late_game"))
        .unwrap();
    assert_eq!(late.verdict, Verdict::Regression);
    assert!(
        rows.iter()
            .filter(|row| row.verdict == Verdict::Regression)
            .count()
            == 1
    );
}

#[test]
fn missing_or_unusable_dev_reference_falls_back_with_a_reason() {
    let temp = tempfile::tempdir().expect("directory");
    let path = temp.path().join("dev-reference.json");
    let reason = dev::load(&path).expect_err("missing");
    assert!(reason.contains("missing"), "{reason}");

    fs::write(
        &path,
        br#"{"unavailable":"no passing dev run with a performance artifact"}"#,
    )
    .unwrap();
    assert!(
        dev::load(&path)
            .expect_err("unavailable")
            .contains("no passing dev run")
    );

    fs::write(&path, b"not json").unwrap();
    assert!(
        dev::load(&path)
            .expect_err("garbage")
            .contains("unreadable")
    );

    // A run measured with other workloads (or before identity was recorded).
    let other = dev_reference(1, BTreeMap::new());
    fs::write(
        &path,
        serde_json::to_vec(&dev::DevFile::Measured(other)).unwrap(),
    )
    .unwrap();
    assert!(
        dev::load(&path)
            .expect_err("identity")
            .contains("different workloads")
    );

    let good = dev_reference(1, dev::identity());
    fs::write(
        &path,
        serde_json::to_vec(&dev::DevFile::Measured(good)).unwrap(),
    )
    .unwrap();
    assert_eq!(dev::load(&path).expect("usable").run, 42);
}

#[test]
fn both_late_game_kernels_count_the_ticks_the_identity_records() {
    let benches = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../simulation/benches");
    let window = format!("for _ in 0..{} {{", dev::LATE_GAME_TICKS);
    for bench in ["instructions.rs", "late_game_instructions.rs"] {
        let text = std::fs::read_to_string(benches.join(bench)).unwrap();
        assert_eq!(text.matches(&window).count(), 1, "{bench}: {window}");
    }
}
