//! Main-session parsing must preserve every denial from the integration judge.
use super::{Fixture, denied, judge};
use crate::agents::{role::Role, shell};
use std::process::Command;

const BYPASSES: [&str; 5] = [
    r#"echo "`sh -c \"cargo build\"`""#,
    r#"echo "`printf \"don't\"; cargo build`""#,
    "echo \"$( ((1 << 2))\nprintf ok)\"; cargo build",
    "cargo build; echo \\",
    "cargo build; echo text\\",
];

fn regression(index: usize, stdout: &str) {
    let command = BYPASSES[index];
    let harmless = command.replace("cargo build", "printf RAN >&2");
    let output = Command::new("bash")
        .args(["--noprofile", "--norc", "-c", &harmless])
        .env_remove("BASH_ENV")
        .output()
        .expect("bash in the tools image");
    assert!(output.status.success(), "bash: {harmless}: {output:?}");
    assert_eq!(output.stdout, stdout.as_bytes(), "bash: {harmless}");
    assert_eq!(output.stderr, b"RAN", "bash: {harmless}");
    let fixture = Fixture::new();
    denied(&fixture, Role::Main, command);
}

#[test]
fn double_quoted_backtick_unescapes_shell_script_quotes() {
    regression(0, "\n");
}

#[test]
fn double_quoted_backtick_unescapes_quotes_around_an_apostrophe() {
    regression(1, "don't\n");
}

#[test]
fn nested_arithmetic_is_not_a_heredoc_hiding_the_next_command() {
    regression(2, "ok\n");
}

#[test]
fn eof_backslash_is_a_literal_word() {
    regression(3, "\\\n");
}

#[test]
fn eof_backslash_after_text_is_literal() {
    regression(4, "text\\\n");
}

#[test]
fn integration_accepted_eof_quotes_do_not_hide_earlier_commands() {
    let fixture = Fixture::new();
    for command in [
        "cargo build; echo $'unterminated",
        "cargo build; echo $'ending\\",
    ] {
        denied(&fixture, Role::Main, command);
    }
}

#[test]
fn backtick_decoding_keeps_the_integration_verdict_floor() {
    let fixture = Fixture::new();
    // Bash's unquoted backtick decoding retains these backslashes. The old
    // judge removed them; its stricter script verdict must still hold.
    denied(&fixture, Role::Main, r#"echo `sh -c \"cargo build\"`"#);
}

#[test]
fn failed_nested_spans_keep_outer_commands_and_queue_the_raw_body() {
    let fixture = Fixture::new();
    for (command, body) in [
        ("echo ${x:-$'a\\'}; cargo build", "x:-$'a\\'"),
        (
            "echo \"$( ((1 << 2))\nprintf ok)\"; cargo build",
            " ((1 << 2))\nprintf ok",
        ),
        (
            "echo <( ((1 << 2))\nprintf ok); cargo build",
            " ((1 << 2))\nprintf ok",
        ),
        (
            "echo `printf \\\"don't\\\"; cargo build`",
            "printf \"don't\"; cargo build",
        ),
    ] {
        let parsed = shell::parse(command).expect("old span boundary is accepted");
        assert!(parsed.substitutions.iter().any(|s| s == body), "{command}");
        denied(&fixture, Role::Main, command);
    }
    // Even malformed arithmetic accepted by the integration lexer must not
    // hide a subsequent command from the main-session policy.
    denied(&fixture, Role::Main, "echo $(( 1 ); cargo build");
}

#[test]
fn main_session_verdicts_never_weaken_integration_verdicts() {
    // Captured through agent-hook at origin/dev
    // 12486ad76250a1af16fba5c5b7b70f53eda857ce, role main. Each JSONL row is
    // [source, denied]. Corpus: every string literal in bash, dispatch, vcs,
    // shell_state and shell_lexer tests, plus the latter's generated command
    // forms and these regressions. No old lexer is linked into the tests.
    let fixture = Fixture::new();
    let cases = include_str!("parse_failures/verdicts_0.jsonl")
        .lines()
        .chain(include_str!("parse_failures/verdicts_1.jsonl").lines())
        .map(|row| serde_json::from_str::<(String, bool)>(row).expect("verdict data"))
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 576);
    for command in BYPASSES {
        assert!(cases.iter().any(|(s, denied)| s == command && *denied));
    }
    let weakened = cases
        .iter()
        .filter_map(|(command, dev_denied)| {
            let denied = judge(&fixture, Role::Main, command).is_err();
            (*dev_denied && !denied).then_some(command)
        })
        .collect::<Vec<_>>();
    assert!(
        weakened.is_empty(),
        "integration denials lost: {weakened:#?}"
    );
}
