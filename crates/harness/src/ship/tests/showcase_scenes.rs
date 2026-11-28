//! The timing, api and agent-session scenes, the side-by-side terminal and
//! the session converter (docs/showcase.md).
mod findings;
mod session_text;
use super::super::showcase::{
    Origins, Scene, Storyboard, durations, plan, session, session::convert, session::redact,
};

fn board(scene: &str) -> String {
    format!(r#"{{"title":"t","scenes":[{scene}]}}"#)
}

fn refuse(scene: &str, why: &str) {
    let error = Storyboard::parse(&board(scene)).unwrap_err();
    assert!(error.contains(why), "{error}");
}

const TIMING: &str = r#"{"kind":"timing","label":"make test","command":"make test-unit",
  "before_ms":24000,"after_ms":10000,"extra":[{"label":"RSS","before":"2 GB","after":"1 GB"},
  {"label":"cache hits","before":"0","after":"41"}]}"#;
const API: &str = r#"{"kind":"api","caption":"The ping reply","method":"GET","path":"/ab",
  "before":{"status":200,"body":"{\n  \"v\": 1\n}"},"after":{"status":200,"body":"{\n  \"v\": 2\n}"}}"#;
const SESSION: &str = r#"{"kind":"agent-session","tag":"claude -p","caption":"A push is refused",
  "steps":[{"role":"user","text":"abc"}]}"#;
const COMPARE: &str = r#"{"kind":"terminal","tag":"t","caption":"c","compare":{
  "before":[{"style":"cmd","text":"ls"}],"after":[{"style":"out","text":"x"}]}}"#;

#[test]
fn the_new_scene_kinds_parse_with_their_defaults() {
    let timing = Storyboard::parse(&board(TIMING)).unwrap();
    let Scene::Timing { unit, extra, .. } = &timing.scenes[0] else {
        panic!("a timing scene");
    };
    assert_eq!(*unit, super::super::showcase::Unit::Ms);
    assert_eq!(extra.len(), 2);
    let api = Storyboard::parse(&board(API)).unwrap();
    assert!(matches!(&api.scenes[0], Scene::Api { request: None, .. }));
    let session = Storyboard::parse(&board(SESSION)).unwrap();
    assert!(matches!(&session.scenes[0], Scene::AgentSession { .. }));
    let compare = Storyboard::parse(&board(COMPARE)).unwrap();
    assert!(matches!(
        &compare.scenes[0],
        Scene::Terminal {
            compare: Some(_),
            ..
        }
    ));
    let narrated = TIMING.replace("\"command\"", "\"narration\":\"Faster.\",\"command\"");
    let narrated = Storyboard::parse(&board(&narrated)).unwrap();
    assert_eq!(narrated.scenes[0].narration(), Some("Faster."));
}

#[test]
fn scene_lengths_mirror_the_recorders_pacing() {
    let at = |scene: &str| durations(&Storyboard::parse(&board(scene)).unwrap(), &[None])[0];
    // 1000 intro + 2500 race + 2×400 extras + 2500 hold.
    assert_eq!(at(TIMING), 6800);
    // "GET /ab" typed (7×22), 500 pause, 3 body lines at 300 ms, 3000 hold.
    assert_eq!(at(API), 154 + 500 + 900 + 3000);
    // 3 characters at 10 ms and a 400 ms pause, then the hold.
    assert_eq!(at(SESSION), 30 + 400 + 3000);
    // The longer pane sets the length: "x" shown (650) beats "ls" typed (544).
    assert_eq!(at(COMPARE), 650 + 3000);
    // Narration stretches the new kinds like any other.
    let board = Storyboard::parse(&board(TIMING)).unwrap();
    assert_eq!(durations(&board, &[Some(20_000)]), [20_600]);
}

#[test]
fn timing_scenes_are_bounded() {
    // Errors name the scene they come from.
    refuse(
        &TIMING.replace("24000", "0"),
        "storyboard: scene 1 timing before_ms",
    );
    refuse(&TIMING.replace("24000", "0"), "before_ms");
    refuse(&TIMING.replace("10000", "3600001"), "after_ms");
    refuse(&TIMING.replace("\"make test\"", "\" \""), "label is empty");
    refuse(
        &TIMING.replace("make test-unit", &"c".repeat(201)),
        "command",
    );
    refuse(
        &TIMING.replace("\"command\"", "\"x\":1,\"command\""),
        "unknown field",
    );
    let many = r#"{"label":"a","before":"1","after":"2"},"#.repeat(5);
    refuse(
        &TIMING.replace("\"extra\":[", &format!("\"extra\":[{many}")),
        "more than 4",
    );
    refuse(&TIMING.replace("2 GB", &"g".repeat(41)), "extra before");
    refuse(&TIMING.replace("make test-unit", "a\\u0007b"), "control");
}

#[test]
fn api_scenes_are_bounded() {
    refuse(&API.replace("GET", "FETCH"), "method");
    refuse(&API.replace("/ab", "ab"), "start with `/`");
    refuse(&API.replace("/ab", "/a b"), "no spaces");
    refuse(
        &API.replace(
            "200,\"body\":\"{\\n  \\\"v\\\": 2",
            "99,\"body\":\"{\\n  \\\"v\\\": 2",
        ),
        "after status",
    );
    let long_line = "x".repeat(81);
    refuse(&API.replace("\\\"v\\\": 2", &long_line), "line over 80");
    let tall = "l\\n".repeat(22);
    refuse(&API.replace("\\\"v\\\": 2", &tall), "over 22 lines");
    let big = "y".repeat(2001);
    refuse(&API.replace("\\\"v\\\": 2", &big), "2000 characters");
    let request = API.replace("\"before\"", "\"request\":\"{\\\"a\\\":1}\",\"before\"");
    assert!(Storyboard::parse(&board(&request)).is_ok());
    refuse(
        &request.replace("{\\\"a\\\":1}", &"r\\n".repeat(21)),
        "request is over 20",
    );
    let empty_after = API.replace("{\\n  \\\"v\\\": 2\\n}", "");
    assert!(Storyboard::parse(&board(&empty_after)).is_ok());
}

#[test]
fn sessions_are_bounded() {
    refuse(
        &SESSION.replace(
            "\"steps\":[{\"role\":\"user\",\"text\":\"abc\"}]",
            "\"steps\":[]",
        ),
        "1 to 40",
    );
    let step = r#"{"role":"agent","text":"x"},"#;
    let many = format!(
        "\"steps\":[{}{{\"role\":\"agent\",\"text\":\"x\"}}]",
        step.repeat(40)
    );
    refuse(
        &SESSION.replace("\"steps\":[{\"role\":\"user\",\"text\":\"abc\"}]", &many),
        "1 to 40",
    );
    refuse(&SESSION.replace("abc", &"a".repeat(601)), "step 1's text");
    refuse(&SESSION.replace("abc", " "), "step 1's text");
    refuse(&SESSION.replace("abc", "a\\u0007"), "control");
    refuse(
        &SESSION.replace("\"text\"", "\"verdict\":\"denied\",\"text\""),
        "only tool and hook",
    );
    let hook = SESSION
        .replace("\"user\"", "\"hook\"")
        .replace("\"text\"", "\"verdict\":\"denied\",\"text\"");
    assert!(Storyboard::parse(&board(&hook)).is_ok());
    let text = "z".repeat(600);
    let eleven = format!(r#"{{"role":"agent","text":"{text}"}},"#).repeat(10);
    let steps = format!("\"steps\":[{eleven}{{\"role\":\"agent\",\"text\":\"z\"}}]");
    refuse(
        &SESSION.replace("\"steps\":[{\"role\":\"user\",\"text\":\"abc\"}]", &steps),
        "at most 6000",
    );
    refuse(&SESSION.replace("claude -p", ""), "session tag");
}

#[test]
fn terminals_show_lines_or_a_comparison_never_both_or_neither() {
    let bare = board(r#"{"kind":"terminal","tag":"t","caption":"c"}"#);
    assert!(Storyboard::parse(&bare).is_ok());
    let both = COMPARE.replace(
        "\"compare\"",
        "\"lines\":[{\"style\":\"out\",\"text\":\"q\"}],\"compare\"",
    );
    refuse(&both, "not both");
    refuse(
        &COMPARE.replace("[{\"style\":\"cmd\",\"text\":\"ls\"}]", "[]"),
        "before pane has 1 to 30",
    );
    let long = COMPARE.replace("\"ls\"", &format!("\"{}\"", "l".repeat(101)));
    refuse(&long, "line over 100");
    let tall = format!(
        "[{}]",
        r#"{"style":"out","text":"q"},"#.repeat(31).trim_end_matches(',')
    );
    refuse(
        &COMPARE.replace("[{\"style\":\"out\",\"text\":\"x\"}]", &tall),
        "after pane has 1 to 30",
    );
    refuse(
        &COMPARE.replace("\"compare\"", "\"layout\":\"x\",\"compare\""),
        "layout",
    );
}

#[test]
fn browser_pairs_cannot_be_laid_out_side_by_side() {
    // Documented limit: the recorder films one page in one context.
    let scene =
        r#"{"kind":"browser","path":"/","caption":"c","seconds":2,"layout":"side-by-side"}"#;
    assert!(
        Storyboard::parse(&board(scene))
            .unwrap_err()
            .contains("layout")
    );
}

#[test]
fn the_plan_carries_the_new_scenes_to_the_recorder() {
    let text = format!("{{\"title\":\"t\",\"scenes\":[{TIMING},{API},{SESSION},{COMPARE}]}}");
    let parsed = Storyboard::parse(&text).unwrap();
    let planned = durations(&parsed, &[None; 4]);
    let json = serde_json::to_value(plan(&parsed, &planned, &Origins::default()).unwrap()).unwrap();
    let kinds: Vec<_> = json["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|scene| scene["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["timing", "api", "agent-session", "terminal"]);
    assert_eq!(json["scenes"][0]["before_ms"], 24000);
    assert_eq!(json["scenes"][0]["duration_ms"], 6800);
    assert_eq!(json["scenes"][3]["compare"]["after"][0]["text"], "x");
}

#[test]
fn the_recorder_draws_the_new_scenes_from_data_alone() {
    let record = include_str!("../showcase/record.mjs");
    let scenes = include_str!("../showcase/scenes.mjs");
    for kind in ["timing", "api", "agent-session"] {
        assert!(
            record.contains(&format!("scene.kind === \"{kind}\"")),
            "{kind}"
        );
    }
    assert!(record.contains("if (scene.compare) return compare("));
    assert!(record.contains("from \"./scenes.mjs\""));
    // Pure drawing: no navigation, network or file access in the scene module.
    for forbidden in [
        "goto(",
        "fetch(",
        "import(",
        "XMLHttpRequest",
        "WebSocket",
        "node:",
        "require(",
    ] {
        assert!(!scenes.contains(forbidden), "{forbidden}");
    }
    assert!(scenes.contains("faster.toFixed(1)}× faster"));
    assert!(scenes.contains("requestAnimationFrame"));
    assert!(scenes.contains("row.className = `row${marks[side][i]"));
    assert!(scenes.contains("\"✗ DENIED\"") || scenes.contains("✗ DENIED"));
    // Pacing constants mirror scenes.rs.
    for (name, value) in [
        ("TYPE_MS", "22"),
        ("SESSION_TYPE_MS", "10"),
        ("SESSION_PAUSE_MS", "400"),
        ("RACE_MS", "2500"),
        ("TIMING_INTRO_MS", "1000"),
        ("EXTRA_MS", "400"),
        ("REPLY_LINE_MS", "300"),
        ("REQUEST_PAUSE_MS", "500"),
    ] {
        assert!(
            scenes.contains(&format!("const {name} = {value};")),
            "{name}"
        );
    }
    let work = tempfile::tempdir().unwrap();
    super::super::showcase::write_recorder(work.path()).unwrap();
    assert!(work.path().join("record.mjs").is_file());
    assert!(work.path().join("scenes.mjs").is_file());
}

const CLAUDE_LOG: &str = r#"not json at all
{"type":"system","subtype":"init"}
{"type":"user","message":{"role":"user","content":"Push the branch with token=abcd1234"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Pushing."},{"type":"tool_use","name":"Bash","input":{"command":"git push origin dev"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":[{"type":"text","text":"PreToolUse hook denied: push dev"}]}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"a.rs"}},{"type":"tool_use","name":"Odd","input":{"x":1}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"fine"},{"type":"tool_result","is_error":true,"content":"exit 1: boom"},{"type":"text","text":"also user"}]}}
{"type":"result","result":"done"}"#;

#[test]
fn a_claude_log_becomes_bounded_steps() {
    use super::super::showcase::{Role, Verdict};
    let steps = convert(CLAUDE_LOG).unwrap();
    let shape: Vec<_> = steps.iter().map(|s| (s.role, s.verdict)).collect();
    assert_eq!(
        shape,
        [
            (Role::User, None),
            (Role::Agent, None),
            (Role::Tool, None),
            (Role::Hook, Some(Verdict::Denied)),
            (Role::Tool, None),
            (Role::Tool, None),
            (Role::Tool, None),
            (Role::User, None),
        ]
    );
    assert_eq!(steps[0].text, "Push the branch with token=[redacted]");
    assert_eq!(steps[2].text, "Bash: git push origin dev");
    assert_eq!(steps[4].text, "Read: a.rs");
    assert_eq!(steps[5].text, "Odd: {\"x\":1}");
    assert_eq!(steps[6].text, "exit 1: boom");
    // A converted log is a valid scene.
    let scene = serde_json::json!({"kind":"agent-session","tag":"t","caption":"c","steps":steps});
    Storyboard::parse(&board(&scene.to_string())).unwrap();
}

#[test]
fn a_codex_log_becomes_steps() {
    let log = r#"{"type":"thread.started","thread_id":"x"}
{"type":"item.completed","item":{"type":"reasoning","text":"hm"}}
{"type":"item.completed","item":{"type":"agent_message","text":"Running tests."}}
{"type":"item.completed","item":{"type":"command_execution","command":"make test","status":"completed"}}
{"type":"item.completed","item":{"type":"command_execution","command":"git push","status":"declined"}}
{"type":"turn.completed"}"#;
    let steps = convert(log).unwrap();
    let texts: Vec<_> = steps.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Running tests.",
            "$ make test",
            "$ git push",
            "command declined"
        ]
    );
    assert_eq!(
        steps[3].verdict,
        Some(super::super::showcase::Verdict::Denied)
    );
}

#[test]
fn conversion_is_bounded_and_refuses_other_logs() {
    assert!(convert("").unwrap_err().contains("no user, agent"));
    assert!(convert("{\"type\":\"system\"}").is_err());
    let line = |text: &str| {
        format!(
            "{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{text}\"}}}}\n"
        )
    };
    let many: String = (0..60).map(|i| line(&format!("step {i}"))).collect();
    assert_eq!(convert(&many).unwrap().len(), 40);
    let long = line(&"w ".repeat(400));
    let one = convert(&long).unwrap();
    assert_eq!(one[0].text.chars().count(), 600);
    assert!(one[0].text.ends_with('…'));
    // Total text is capped: eleven 600-character steps do not fit in 6000.
    let heavy: String = (0..11).map(|_| line(&"v ".repeat(300))).collect();
    assert_eq!(convert(&heavy).unwrap().len(), 10);
}

#[test]
fn secret_looking_strings_are_redacted() {
    for (input, expected) in [
        (
            "export API_KEY=hunter2 && go",
            "export API_KEY=[redacted] && go",
        ),
        (
            "curl -H 'Authorization: Bearer abc.def' x",
            "curl -H 'Authorization: Bearer [redacted]' x",
        ),
        ("Bearer s3cr3t more", "Bearer [redacted] more"),
        (
            r#"{"password": "p@ss", "name": "ok"}"#,
            r#"{"password": "[redacted]", "name": "ok"}"#,
        ),
        ("use sk-ant-api03-abcdefghijkl now", "use [redacted] now"),
        ("ghp_abcdefghijklmnop", "[redacted]"),
        ("id 0123456789abcdef0123456789abcdef01", "id [redacted]"),
        (
            "see /home/anko/Work/projects/AoeWorld/crates/harness/src/ship/showcase/mod.rs ok",
            "see /home/anko/Work/projects/AoeWorld/crates/harness/src/ship/showcase/mod.rs ok",
        ),
        ("sk-short and plain words", "sk-short and plain words"),
        ("monkey see", "monkey see"),
    ] {
        assert_eq!(redact(input), expected);
    }
}

#[test]
fn the_session_command_reads_one_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("s.jsonl");
    std::fs::write(&log, CLAUDE_LOG).unwrap();
    session::run(&log).unwrap();
    let link = dir.path().join("link.jsonl");
    std::os::unix::fs::symlink(&log, &link).unwrap();
    assert!(
        session::run(&link)
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
    assert!(session::run(dir.path()).is_err());
    let other = dir.path().join("other.jsonl");
    std::fs::write(&other, "{\"type\":\"system\"}\n").unwrap();
    assert!(session::run(&other).is_err());
    let bytes = std::fs::File::create(dir.path().join("big.jsonl")).unwrap();
    bytes.set_len(8 * 1024 * 1024 + 1).unwrap();
    assert!(
        session::run(&dir.path().join("big.jsonl"))
            .unwrap_err()
            .to_string()
            .contains("limit")
    );
    std::fs::write(dir.path().join("bin.jsonl"), [0xff, 0xfe]).unwrap();
    assert!(
        session::run(&dir.path().join("bin.jsonl"))
            .unwrap_err()
            .to_string()
            .contains("UTF-8")
    );
}
