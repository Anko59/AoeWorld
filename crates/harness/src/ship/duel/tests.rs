use super::*;
use crate::{
    agents::{Runtime, launch},
    ship::duel::runner,
};
use std::{collections::BTreeMap, time::Duration};

#[test]
fn bounded_task_and_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let task = temp.path().join("task");
    fs::write(&task, "Task\nDetails").unwrap();
    assert_eq!(read_task(&task).unwrap(), "Task\nDetails");
    fs::write(&task, vec![b'x'; 65536]).unwrap();
    assert!(read_task(&task).is_ok());
    fs::write(&task, vec![b'x'; 65537]).unwrap();
    assert!(read_task(&task).unwrap_err().contains("64 KiB"));
    assert!(read_task(temp.path()).unwrap_err().contains("regular"));
    assert!(read_task(&temp.path().join("missing")).is_err());
    fs::write(&task, "\n").unwrap();
    assert!(read_task(&task).is_err());
    fs::write(&task, [255]).unwrap();
    assert!(read_task(&task).is_err());
    let config = |values: &[(&str, &str)]| {
        let map: BTreeMap<_, _> = values.iter().copied().collect();
        Options::from_env(|k| map.get(k).map(|s| s.to_string()))
    };
    assert!(config(&[]).unwrap_err().contains("required"));
    assert!(
        config(&[("CLAUDECODE", "1"), ("CODEX_THREAD_ID", "id")])
            .unwrap_err()
            .contains("ambiguous")
    );
    assert!(config(&[("CODEX_CI", "1"), ("CODEX_THREAD_ID", "id")]).is_ok());
    for runtime in ["claude", "codex", "pi"] {
        assert!(config(&[("DUEL_RUNTIME", runtime)]).is_ok());
    }
    for marker in [
        "CLAUDECODE",
        "CODEX_THREAD_ID",
        "CODEX_CI",
        "PI_CODING_AGENT",
    ] {
        assert!(config(&[(marker, "1")]).is_ok());
    }
    for (key, value) in [
        ("DUEL_RUNTIME", "dsh"),
        ("DUEL_RUNTIME", "bad"),
        ("DUEL_ROUNDS", "0"),
        ("DUEL_ROUNDS", "5"),
        ("DUEL_ROUNDS", "bad"),
        ("DUEL_BUDGET_MINUTES", "0"),
    ] {
        assert!(config(&[("DUEL_RUNTIME", "codex"), (key, value)]).is_err());
    }
    let options = config(&[
        ("DUEL_RUNTIME", "pi"),
        ("DUEL_MODEL", "p/m"),
        ("DUEL_EFFORT", "high"),
        ("DUEL_ROUNDS", "4"),
        ("DUEL_CHECK", "make test-unit"),
        ("DUEL_BUDGET_MINUTES", "2"),
    ])
    .unwrap();
    assert_eq!(options.rounds, 4);
    assert_eq!(options.budget, Duration::from_secs(120));
    assert_eq!(options.model.as_deref(), Some("p/m"));
}

#[test]
fn work_launch_has_hooks_writes_model_effort_and_no_smoke_limit() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".codex")).unwrap();
    fs::write(
        root.path().join(".codex/hooks.json"),
        r#"{"hooks":{"PreToolUse":[]}}"#,
    )
    .unwrap();
    fs::create_dir_all(root.path().join(".claude/agents")).unwrap();
    fs::write(root.path().join(".claude/agents/tester.md"), "tester").unwrap();
    for runtime in [Runtime::Claude, Runtime::Codex, Runtime::Pi] {
        let c = launch::work(
            runtime,
            root.path(),
            "tester",
            "task",
            Some("provider/model"),
            Some("high"),
        )
        .unwrap();
        let args: Vec<_> = c
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.contains(&"high".into())
                || args
                    .iter()
                    .any(|a| a.contains("model_reasoning_effort=\"high\""))
        );
        assert_eq!(c.get_current_dir(), Some(root.path()));
        assert!(
            c.get_envs()
                .any(|(k, v)| k == "AOE_AGENT_ROLE" && v.unwrap() == "tester")
        );
        assert_eq!(args.last().unwrap(), "task");
        match runtime {
            Runtime::Claude => {
                assert!(args.contains(&"--agent".into()));
                assert!(args.contains(&"200".into()));
                assert!(args.iter().any(|a| a.contains("Edit,Write")));
            }
            Runtime::Codex => {
                assert!(args.contains(&"danger-full-access".into()));
                assert!(args.iter().any(|a| a.starts_with("hooks.PreToolUse=")));
            }
            Runtime::Pi => {
                assert!(args.contains(&"--provider".into()));
                assert!(args.contains(&"-a".into()));
            }
            _ => unreachable!(),
        }
    }
    assert!(
        launch::work(Runtime::Dsh, root.path(), "tester", "task", None, None)
            .unwrap_err()
            .contains("not supported")
    );
    for runtime in [Runtime::Claude, Runtime::Pi] {
        assert!(launch::work(runtime, root.path(), "implementer", "task", None, None).is_ok());
    }
    assert!(
        launch::work(
            Runtime::Pi,
            root.path(),
            "tester",
            "task",
            Some("model"),
            None
        )
        .is_ok()
    );
    assert_eq!(
        runner::default_check(&["x.spec.ts".into()]),
        "make test-e2e"
    );
    assert_eq!(
        runner::default_check(&["tests/data".into()]),
        "make test-unit"
    );
    assert_eq!(
        runner::default_check(&["tests/a.rs".into(), "x.spec.ts".into()]),
        "make test-unit"
    );
}

#[test]
fn runtime_last_reports_extract_assistant_text() {
    for (runtime, text) in [
        (Runtime::Claude, r#"{"result":"last report"}"#),
        (
            Runtime::Codex,
            r#"{"item":{"type":"agent_message","text":"last report"}}"#,
        ),
        (
            Runtime::Pi,
            r#"{"message":{"role":"assistant","content":[{"text":"last report"}]}}"#,
        ),
    ] {
        assert_eq!(runner::report(runtime, text), "last report");
    }
    assert_eq!(runner::report(Runtime::Dsh, "plain report"), "plain report");
}

#[test]
fn prompt_tails_are_bounded_at_utf8_boundaries() {
    let output = "界".repeat(10000);
    let tail = runner::bounded(&output);
    assert!(tail.len() <= 8000);
    assert!(output.ends_with(&tail));
    assert_eq!(runner::bounded("small"), "small");
}
