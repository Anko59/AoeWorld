use super::*;
use crate::qa::{Finding, Journey, REQUIRED};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn report(evidence: &Path) -> Report {
    Report {
        version: 1,
        budget: "fast".into(),
        build: "claimed-build".into(),
        scenario: "claimed-scenario".into(),
        status: Status::Pass,
        journeys: REQUIRED
            .iter()
            .map(|name| Journey {
                name: (*name).into(),
                completed: true,
                evidence: vec![evidence.to_str().unwrap().into()],
            })
            .collect(),
        findings: vec![],
    }
}
fn fixture() -> (TempDir, PathBuf, PathBuf, PathBuf, Report) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("qa");
    fs::create_dir(&root).unwrap();
    let artifact = root.join("screen.bin");
    fs::write(&artifact, b"abc").unwrap();
    let input = root.join("session.json");
    let report = report(&artifact);
    save(&input, &report);
    (owner, root, input, artifact, report)
}
fn save(path: &Path, report: &Report) {
    fs::write(path, serde_json::to_vec(report).unwrap()).unwrap();
}
fn rejected(root: &Path, input: &Path, report: &mut Report, path: &Path) {
    report.journeys[0].evidence = vec![path.to_str().unwrap().into()];
    save(input, report);
    assert!(
        observe_file_at(input, root).is_err(),
        "must reject {}",
        path.display()
    );
}
#[test]
fn measured_raw_bytes_purposes_and_claimed_pass_never_authenticate_execution() {
    let (_owner, root, input, artifact, report) = fixture();
    let first = observe_file_at(&input, &root).unwrap();
    assert_eq!(
        first.assessment,
        "STRUCTURAL_EVIDENCE_OBSERVED_NON_AUTHORITATIVE"
    );
    assert!(!first.authoritative);
    assert_eq!(first.claimed_status, Status::Pass);
    assert_eq!(first.independent_qa, "NOT_ASSESSED");
    assert_eq!(first.journey_execution, "NOT_ASSESSED");
    assert_eq!(first.source_identity, "UNAVAILABLE");
    assert_eq!(first.served_build_binding, "UNAVAILABLE");
    assert_eq!(first.artifacts.len(), 1);
    assert_eq!(first.artifacts[0].path, "screen.bin");
    assert_eq!(first.artifacts[0].purposes.len(), REQUIRED.len());
    assert_eq!(first.artifacts[0].bytes, 3);
    assert_eq!(
        first.artifacts[0].raw_blake3,
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    assert!(
        first
            .limits
            .iter()
            .any(|value| value.contains("shared evidence"))
    );
    let text = serde_json::to_string(&first).unwrap();
    assert!(!text.contains("claimed-build"));
    assert!(!text.contains("claimed-scenario"));
    assert!(!text.contains(root.to_str().unwrap()));
    fs::write(&artifact, [0xff, 0x00, 0x80]).unwrap();
    let changed = observe_file_at(&input, &root).unwrap();
    assert_ne!(
        changed.artifacts[0].raw_blake3,
        first.artifacts[0].raw_blake3
    );
    assert_ne!(changed.manifest_blake3, first.manifest_blake3);
    // Report filename/physical inode are outside the logical payload.
    let other = root.join("scratch.json");
    save(&other, &report);
    assert_eq!(
        observe_file_at(&input, &root).unwrap(),
        observe_file_at(&other, &root).unwrap()
    );
}
#[test]
fn actual_purpose_associations_and_finite_variants_change_the_generated_manifest() {
    let (_owner, root, input, artifact, mut report) = fixture();
    let second = root.join("other.bin");
    fs::write(&second, b"abc").unwrap();
    report.journeys[0].evidence = vec![second.to_str().unwrap().into()];
    save(&input, &report);
    let one = observe_file_at(&input, &root).unwrap();
    report.journeys[1].evidence = report.journeys[0].evidence.clone();
    report.journeys[0].evidence = vec![artifact.to_str().unwrap().into()];
    save(&input, &report);
    let two = observe_file_at(&input, &root).unwrap();
    assert_ne!(one.manifest_blake3, two.manifest_blake3);
    assert_ne!(one.artifacts, two.artifacts);
    for value in 0..16u8 {
        let bytes = vec![value; usize::from(value) + 1];
        fs::write(&artifact, &bytes).unwrap();
        let measured = observe_file_at(&input, &root).unwrap();
        assert_eq!(
            measured
                .artifacts
                .iter()
                .find(|item| item.path == "screen.bin")
                .unwrap()
                .raw_blake3,
            blake3::hash(&bytes).to_hex().to_string()
        );
        let mut altered = measured.artifacts.clone();
        altered[0].purposes.clear();
        assert_ne!(
            measured.manifest_blake3,
            manifest(&measured.report, &altered).unwrap()
        );
        assert_eq!(measured, observe_file_at(&input, &root).unwrap());
    }
    // Path relocation changes absolute paths in raw report bytes: never promise whole-report portability.
    let (_owner2, root2, input2, _, _) = fixture();
    let relocated = observe_file_at(&input2, &root2).unwrap();
    let (_owner3, root3, input3, _, _) = fixture();
    let fresh = observe_file_at(&input3, &root3).unwrap();
    assert_eq!(relocated.artifacts, fresh.artifacts);
    assert_ne!(relocated.report.raw_blake3, fresh.report.raw_blake3);
    assert_ne!(relocated.manifest_blake3, fresh.manifest_blake3);
}
#[test]
fn duplicate_paths_unknown_fields_bad_text_budgets_and_closed_report_parse_reject() {
    let (_owner, root, input, artifact, mut report) = fixture();
    report.journeys[0]
        .evidence
        .push(artifact.to_str().unwrap().into());
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_err());
    report.journeys[0].evidence.pop();
    for bad in ["", "fast\n", "unlimited"] {
        report.budget = bad.into();
        save(&input, &report);
        assert!(observe_file_at(&input, &root).is_err());
    }
    report.budget = "fast".into();
    for bad in [" ".into(), "control\0".into(), "X".repeat(4097)] {
        report.build = bad;
        save(&input, &report);
        assert!(observe_file_at(&input, &root).is_err());
    }
    report.build = "claim".into();
    report.status = Status::Findings;
    report.findings.push(Finding {
        title: "title".into(),
        reproduction: "steps".into(),
        expected: "expected".into(),
        actual: "actual".into(),
        evidence: vec!["".into()],
    });
    save(&input, &report);
    assert!(observe_file_at(&input, &root).is_err());
    report.findings[0].evidence = vec![artifact.to_str().unwrap().into()];
    save(&input, &report);
    let findings = observe_file_at(&input, &root).unwrap();
    assert_eq!(findings.claimed_status, Status::Findings);
    assert!(
        findings.artifacts[0]
            .purposes
            .contains(&Purpose::Finding { index: 0 })
    );
    for pointer in ["/", "/journeys/0", "/findings/0"] {
        let mut value = serde_json::to_value(&report).unwrap();
        if pointer == "/" {
            value["approved"] = true.into();
        } else {
            value.pointer_mut(pointer).unwrap()["approved"] = true.into();
        }
        fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(observe_file_at(&input, &root).is_err());
    }
    for bytes in [
        b"{\"version\":1,\"version\":1}".as_slice(),
        b"{\"journeys\":[{\"name\":\"n\",\"name\":\"n\"}]}",
        b"{} trailing",
        &[0xff],
    ] {
        fs::write(&input, bytes).unwrap();
        assert!(observe_file_at(&input, &root).is_err());
    }
}
mod paths;
