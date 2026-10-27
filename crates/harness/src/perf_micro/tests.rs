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
            },
            {
                "tool": "DHAT",
                "summaries": {"total": {"summary": {
                    "Dhat": {
                        "TotalBytes": {"metrics": {"Both": [{"Int": 80}]}},
                        "TotalBlocks": {"metrics": {"Both": [{"Int": 2}]}}
                    }
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
    let (name, counts) = sample(&line("kernel", 100)).expect("sample");
    assert_eq!(name, "kernel");
    assert_eq!(
        (
            counts.instructions,
            counts.allocation_bytes,
            counts.allocation_count
        ),
        (100, 80, 2)
    );

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
    for name in ["a", "b", "c", "d", "e"] {
        observed.insert(
            name.to_owned(),
            Metrics {
                instructions: 100,
                allocation_bytes: 80,
                allocation_count: 2,
            },
        );
    }
    assert!(proposal(&BTreeMap::new()).is_err());
    let baseline = proposal(&observed).expect("proposal");
    assert!(compare_observed(baseline, BTreeMap::new()).is_err());

    let mut baseline = proposal(&observed).expect("proposal");
    baseline.toolchain = "different".to_owned();
    assert!(compare_observed(baseline, observed.clone()).is_err());

    let baseline = proposal(&observed).expect("proposal");
    observed.get_mut("a").expect("case").instructions = 126;
    let results = compare_observed(baseline, observed).expect("comparison");
    assert_eq!(results.len(), 15);
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
            "{}\n{}\n{}\n",
            line("a", 100),
            line("b", 100),
            line("e", 100)
        ),
    )
    .expect("simulation samples");
    fs::write(&protocol, format!("{}\n", line("c", 100))).expect("protocol samples");
    fs::write(&assets, format!("{}\n", line("d", 100))).expect("asset samples");
    assert_eq!(
        observed_samples_at(&simulation, &protocol, &assets)
            .expect("samples")
            .len(),
        5
    );
    let original =
        proposal(&observed_samples_at(&simulation, &protocol, &assets).expect("samples"))
            .expect("baseline");
    let original_bytes = serde_json::to_vec(&original).expect("JSON");
    fs::write(&baseline, &original_bytes).expect("baseline");
    propose_at(&simulation, &protocol, &assets, &proposed).expect("proposal");
    assert_eq!(fs::read(&baseline).expect("baseline"), original_bytes);
    assert_eq!(
        comparisons_at(&baseline, &simulation, &protocol, &assets)
            .expect("comparison")
            .len(),
        15
    );
    assert!(proposed.is_file());
    fs::write(&protocol, format!("{}\n", line("a", 100))).expect("duplicate");
    assert!(observed_samples_at(&simulation, &protocol, &assets).is_err());
}

#[test]
fn late_game_identity_and_complete_case_set_are_required() {
    let observed: BTreeMap<_, _> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                Metrics {
                    instructions: 100,
                    allocation_bytes: 80,
                    allocation_count: 2,
                },
            )
        })
        .collect();
    let mut baseline = proposal(&observed).unwrap();
    baseline.late_game_workload_hash = "different workload".into();
    assert!(compare_observed(baseline, observed.clone()).is_err());
    let mut baseline = proposal(&observed).unwrap();
    baseline.instructions.remove("e");
    baseline.allocation_bytes.remove("e");
    baseline.allocation_count.remove("e");
    let mut partial = observed;
    partial.remove("e");
    assert!(compare_observed(baseline, partial).is_err());
}
