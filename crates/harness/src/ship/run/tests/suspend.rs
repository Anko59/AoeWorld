//! Real run_one/capture and Docker cleanup, isolated in test-binary subprocesses.
use super::*;
use std::{io::Write, os::unix::fs::PermissionsExt, process::Command};

const ROLE: &str = "AOE_TEST_SHIP_SUSPEND_ROLE";

#[test]
fn role() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let dir = std::env::current_dir().unwrap();
    let (retry, waits) = policy(true, OVERLOADED);
    let command = match role.as_str() {
        "overload-suspend" | "overload-stop-fails" => {
            "echo run >> count; case $(wc -l < count) in 1) sleep 3;; 2) sleep 0.1; echo 300 > gap; sleep 3;; *) test -f stopped;; esac"
        }
        "stop-fails" => {
            "echo run >> count; if [ $(wc -l < count) -eq 1 ]; then sleep 0.1; echo 300 > gap; sleep 3; fi"
        }
        "suspend-overload-suspend" => {
            "echo run >> count; case $(wc -l < count) in 1) sleep 0.1; echo 300 > gap; sleep 3;; 2) sleep 3;; *) sleep 0.1; echo 600 > gap; sleep 3;; esac"
        }
        _ => panic!("unknown role: {role}"),
    };
    let result = run_one(&dir, command, &retry);
    let calls = fs::read_to_string(dir.join("docker-calls")).unwrap_or_default();
    println!(
        "{role}: {}\nruns={} waits={}\nDocker calls:\n{calls}",
        result.line(),
        runs(&dir),
        waits.load(Ordering::SeqCst)
    );
    match role.as_str() {
        "overload-suspend" => {
            assert_eq!(result.verdict, GateVerdict::Pass, "{}", result.line());
            assert_eq!(runs(&dir), 3);
            assert_eq!(waits.load(Ordering::SeqCst), 1);
            assert_eq!(calls.matches("rm -f frozen-one").count(), 1);
            assert!(result.attempts.first().unwrap().timed_out);
            assert_eq!(result.attempts.last().unwrap().verdict, GateVerdict::Pass);
        }
        "stop-fails" | "overload-stop-fails" => {
            assert_eq!(
                result.verdict,
                GateVerdict::Unavailable,
                "{}",
                result.line()
            );
            assert!(result.summary.contains("frozen-one"), "{}", result.summary);
            assert!(
                result.summary.contains("removal refused"),
                "{}",
                result.summary
            );
            assert_eq!(
                runs(&dir),
                if role == "stop-fails" { 1 } else { 2 },
                "no run on top of a surviving container"
            );
            assert!(!dir.join("stopped").exists(), "container still running");
        }
        "suspend-overload-suspend" => {
            assert_eq!(result.verdict, GateVerdict::Fail);
            assert!(
                result.summary.contains("host suspend"),
                "{}",
                result.summary
            );
            assert_eq!(
                runs(&dir),
                3,
                "suspend allowance is shared by the whole gate"
            );
            assert_eq!(waits.load(Ordering::SeqCst), 1);
            assert_eq!(calls.matches("rm -f frozen-one").count(), 1);
        }
        _ => unreachable!(),
    }
}

fn scenario(role: &str) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("gap"), "0").unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    // Only this child's PATH sees the stub. Recovery must enumerate and remove the named
    // interrupted container before the next command can pass (it checks `stopped`).
    let docker = format!(
        "#!/bin/sh\necho \"$*\" >> docker-calls\ncase $1 in\nps) printf 'frozen-one\\t999999999\\n';;\nrm) {};;\n*) exit 1;;\nesac\n",
        if role.contains("fails") {
            "echo 'removal refused' >&2; exit 1"
        } else {
            "touch stopped"
        }
    );
    fs::write(bin.join("docker"), docker).unwrap();
    fs::set_permissions(bin.join("docker"), fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["ship::run::tests::suspend::role", "--exact", "--nocapture"])
        .current_dir(dir.path())
        .env(ROLE, role)
        .env(
            crate::process::suspend::SIMULATED_GAP_FILE,
            dir.path().join("gap"),
        )
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    writeln!(std::io::stdout(), "{stdout}{stderr}").unwrap();
    assert!(output.status.success(), "{role}: {stdout}{stderr}");
}

#[test]
fn a_first_suspend_during_the_overload_retry_stops_containers_and_retries() {
    scenario("overload-suspend");
}

#[test]
fn failed_container_cleanup_reports_unavailable_and_never_retries() {
    scenario("stop-fails");
}

#[test]
fn failed_cleanup_after_the_overload_retry_also_blocks_retrying() {
    scenario("overload-stop-fails");
}

#[test]
fn the_suspend_retry_is_once_per_gate_across_overload_retries() {
    scenario("suspend-overload-suspend");
}
