//! Quote boundaries and expansion bodies must use the shell lexer everywhere.
use super::{Fixture, ROLES, allowed, denied, judge};
use crate::agents::{role::Role, shell};
use std::process::Command;

fn continuation_case(command: &str) {
    let fixture = Fixture::new();
    for role in ROLES {
        if role == Role::Main {
            allowed(&fixture, role, command);
        } else {
            let reason = judge(&fixture, role, command).expect_err("continuation denied");
            assert!(reason.contains("agents do not continue lines"), "{reason}");
        }
    }
}

#[test]
fn ansi_quote_escape_does_not_hide_a_later_continuation() {
    continuation_case("echo $'it\\'s' \\\nx");
}

#[test]
fn substitution_quotes_do_not_hide_a_later_continuation() {
    continuation_case("echo \"$(printf \"don't\")\"; echo a\\\nb");
}

#[test]
fn escaped_space_does_not_start_a_comment() {
    continuation_case("echo a\\ #x\\\nb");
}

#[test]
fn expansion_substitution_boundaries_honour_quoted_parentheses() {
    let fixture = Fixture::new();
    for (command, main_allowed) in [
        ("echo ${x:-$(printf '%s' ')$(cargo build)')}", true),
        ("echo ${x:-$(printf '%s' ')'; cargo build)}", false),
        ("echo ${x:-$(printf '%s' '('; cargo build)}", false),
        ("echo ${x:-$'it\\'s $(cargo build)'}", true),
        ("echo ${x:-$'it\\'s'$(cargo build)}", false),
        ("echo ${x:-${y:-$(cargo build)}}", false),
        ("echo ${x:-a{$(cargo build)}", false),
        ("echo ${x:-`printf '%s' '$(cargo build)'`}", true),
        ("echo $(( $(printf '%s' ')$(cargo build)'; echo 1) ))", true),
        ("echo $(( $(printf '%s' ')'; cargo build) ))", false),
        ("echo $(( '$(cargo build)' ))", false),
        ("echo \"${x:-'$(cargo build)'}\"", false),
        ("echo ${x:-'$(cargo build)'}", true),
    ] {
        for role in ROLES {
            if role == Role::Main && main_allowed {
                allowed(&fixture, role, command);
            } else {
                denied(&fixture, role, command);
            }
        }
    }
}

/// Bash is required in the tools image. Use a harmless printf marker instead
/// of executing any policy-denied command in these differential cases.
#[test]
fn expansion_quote_context_matches_real_bash() {
    let fixture = Fixture::new();
    for (expression, runs) in [
        ("${__aoe_unset:-'$(printf RAN >&2)'}", false),
        ("\"${__aoe_unset:-'$(printf RAN >&2)'}\"", true),
        ("${__aoe_unset:-'`printf RAN >&2`'}", false),
        ("\"${__aoe_unset:-'`printf RAN >&2`'}\"", true),
        ("${__aoe_unset:-$'it\\'s $(printf RAN >&2)'}", false),
        ("${__aoe_unset:-$'it\\'s'$(printf RAN >&2)}", true),
        ("${__aoe_unset:-a{$(printf RAN >&2)}", true),
        ("${__aoe_unset:-$(printf '%s' ')$(printf RAN >&2)')}", false),
        ("$(( $(printf RAN >&2; printf 1) ))", true),
        ("$(( '$(printf RAN >&2; printf 1)' ))", true),
        ("\"$(( '$(printf RAN >&2; printf 1)' ))\"", true),
    ] {
        let command = format!("printf '%s' {expression}");
        let output = Command::new("bash")
            .args(["--noprofile", "--norc", "-c", &command])
            .env_remove("BASH_ENV")
            .env_remove("__aoe_unset")
            .output()
            .expect("bash in the tools image");
        // Arithmetic with literal apostrophes can fail after its command ran.
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.contains("RAN"), runs, "bash: {command}: {stderr}");
        // Only Bash executes the harmless marker. Policy sees a forbidden
        // command in exactly the same quote context and must judge it iff run.
        let policy_command = command.replace("printf RAN >&2", "cargo build");
        assert_eq!(
            judge(&fixture, Role::Main, &policy_command).is_err(),
            runs,
            "policy: {policy_command}"
        );
        allowed(&fixture, Role::Main, &command);
    }
}

#[test]
fn continuation_and_comment_words_match_real_bash() {
    let fixture = Fixture::new();
    for (command, expected, continuation) in [
        ("printf '<%s>' a\\ #x\\\nb", "<a #xb>", true),
        ("printf '<%s>' a\r#x\\\nb", "<a\r#xb>", true),
        ("printf '<%s>' a #x\\\nprintf '<%s>' b", "<a><b>", false),
        ("printf '<%s>' 'a\\\nb'", "<a\\\nb>", false),
        ("printf '<%s>' $'a\\\nb'", "<a\\\nb>", false),
        ("printf '<%s>' \"a\\\nb\"", "<ab>", true),
        ("printf '<%s>' $'it\\'s' \\\nx", "<it's><x>", true),
        (
            "printf '<%s>' \"$(printf \"don't\")\"; printf '<%s>' a\\\nb",
            "<don't><ab>",
            true,
        ),
        ("printf '<%s>' a\\\n#b", "<a#b>", true),
        (
            "printf '<%s>' a;# don't\\\nprintf '<%s>' b",
            "<a><b>",
            false,
        ),
    ] {
        let output = Command::new("bash")
            .args(["--noprofile", "--norc", "-c", command])
            .env_remove("BASH_ENV")
            .output()
            .expect("bash in the tools image");
        assert!(output.status.success(), "bash: {command}: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{command}"
        );
        assert_eq!(
            shell::parse(command).expect("parse").continued_line,
            continuation,
            "{command}"
        );
        for role in ROLES {
            if continuation && role != Role::Main {
                let reason = judge(&fixture, role, command).expect_err("continuation denied");
                assert!(
                    reason.contains("agents do not continue lines"),
                    "{command}: {reason}"
                );
            } else {
                allowed(&fixture, role, command);
            }
        }
    }
}
