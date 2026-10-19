use super::flow::{Script, fixture};
use crate::{
    agents::Runtime,
    review::{Plan, Tier, review_with},
};
use std::{fs, sync::Mutex};

#[test]
fn timeline_journals_blind_and_cross_rounds_with_their_own_boundaries() {
    let temp = fixture("README.md");
    let script = Script {
        first: |p| {
            if p.contains("# Correctness breaker") {
                r#"{"findings":[{"file":"README.md","line":1,"severity":"major","category":"correctness","claim":"wrong text","trigger":"read it","expected_vs_actual":"base vs changed","evidence":"README.md:1"}]}"#.into()
            } else {
                r#"{"findings":[]}"#.into()
            }
        },
        cross: |_| {
            r#"{"verdicts":[{"id":"F1","verdict":"upheld","evidence":"README.md:1"}],"findings":[]}"#.into()
        },
        grade: r#"{"grade":9,"summary":"Change text."}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(
        temp.path(),
        Tier::Medium,
        Runtime::Codex,
        "",
        &Plan::Full,
        &script,
    )
    .unwrap();
    assert_eq!(report.rounds, 2);
    let path = temp.path().join(".git/aoe-ship/timeline/feature.jsonl");
    let text = fs::read_to_string(path).unwrap_or_default();
    let entries: Vec<serde_json::Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let rounds: Vec<_> = entries
        .iter()
        .filter(|e| e["stage"] == "review-round")
        .collect();
    assert_eq!(
        rounds.len(),
        4,
        "one boundary pair per actual round: {text}"
    );
    for (i, kind) in ["blind", "cross"].into_iter().enumerate() {
        let start = rounds[2 * i];
        let end = rounds[2 * i + 1];
        assert_eq!(start["event"], "start");
        assert_eq!(end["event"], "end");
        for entry in [start, end] {
            assert_eq!(entry["detail"]["tier"], "medium");
            assert_eq!(entry["detail"]["round"], i + 1);
            assert_eq!(entry["detail"]["kind"], kind);
            assert_eq!(entry["detail"]["closing"], false);
        }
        assert!(end["t"].as_f64().unwrap() >= start["t"].as_f64().unwrap());
    }
    assert!(rounds[1]["t"].as_f64().unwrap() <= rounds[2]["t"].as_f64().unwrap());
}

#[test]
fn timeline_closing_review_records_only_the_cross_round_it_runs() {
    let temp = fixture("README.md");
    let script = Script {
        first: |_| panic!("closing reviews have no blind round"),
        cross: |_| r#"{"verdicts":[],"findings":[]}"#.into(),
        grade: r#"{"grade":9,"summary":"Fix verified."}"#.into(),
        sessions: Mutex::new(0),
    };
    let plan = Plan::Closing {
        since: crate::ship::git::git(temp.path(), &["rev-parse", "HEAD~1"]).unwrap(),
        prior: vec![],
    };
    let report = review_with(
        temp.path(),
        Tier::Medium,
        Runtime::Codex,
        "",
        &plan,
        &script,
    )
    .unwrap();
    assert_eq!(report.rounds, 1);
    assert_eq!(report.grade, 9);
    let text =
        fs::read_to_string(temp.path().join(".git/aoe-ship/timeline/feature.jsonl")).unwrap();
    let rounds: Vec<serde_json::Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rounds.len(), 2, "{text}");
    for entry in rounds {
        assert_eq!(entry["stage"], "review-round");
        assert_eq!(entry["detail"]["round"], 1);
        assert_eq!(entry["detail"]["kind"], "cross");
        assert_eq!(entry["detail"]["tier"], "medium");
        assert_eq!(entry["detail"]["closing"], true);
    }
}
