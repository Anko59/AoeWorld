use super::*;
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::Stdio,
    time::{Duration, Instant},
};

#[test]
fn sigint_and_sigterm_cancel_gate_run_load_wait_and_release_the_slot() {
    // A one-CPU affinity makes the real host load exceed this runner's core count
    // without generating load on a shared development machine.
    let load: f64 = fs::read_to_string("/proc/loadavg")
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    if load <= 1.0 {
        eprintln!("real load-wait signal rehearsal unavailable: host load is {load}, needs > 1");
        return;
    }
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let cpu = status
        .lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .unwrap()
        .trim()
        .split([',', '-'])
        .next()
        .unwrap();
    for (command, override_value) in [
        ("gate-run", Some("1")),
        ("slot", Some("1")),
        ("gate-run", None),
        ("slot", None),
    ] {
        for signal in [Signal::SIGINT, Signal::SIGTERM] {
            let root = fixture();
            let output = tempfile::tempdir().unwrap();
            let path = root.path().join("gates/registry.json");
            let mut registry: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            registry["gates"][0]["heavy"] = true.into();
            registry["gates"][0]["static"] = false.into();
            fs::write(path, serde_json::to_vec(&registry).unwrap()).unwrap();
            fs::write(
                root.path().join("gates/heavy.json"),
                r#"{"slots":1,"cores":1}"#,
            )
            .unwrap();
            let marker = output.path().join("started");
            let arguments = if command == "gate-run" {
                vec![
                    "gate-run",
                    "--job",
                    "core",
                    "--output",
                    output.path().to_str().unwrap(),
                ]
            } else {
                vec!["slot", "alpha", "--", "touch", marker.to_str().unwrap()]
            };
            let mut child_command = Command::new("taskset");
            child_command
                .args(["-c", cpu, env!("CARGO_BIN_EXE_aoe-harness")])
                .args(arguments)
                .current_dir(root.path())
                .env_remove("AOE_HEAVY_SLOT")
                .env_remove("AOE_HEAVY_DIR")
                .env("AOE_HEAVY_SLOTS", "1")
                .env("AOE_HEAVY_WAIT_MINUTES", "0")
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            match override_value {
                Some(value) => child_command.env("AOE_HEAVY_LOAD_CHECKED", value),
                None => child_command.env_remove("AOE_HEAVY_LOAD_CHECKED"),
            };
            let mut child = child_command.spawn().unwrap();
            let stderr = child.stderr.take().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines() {
                    if tx.send(line.unwrap()).is_err() {
                        break;
                    }
                }
            });
            let start = Instant::now();
            let mut lines = Vec::new();
            let ready = loop {
                match rx.recv_timeout(Duration::from_secs(60).saturating_sub(start.elapsed())) {
                    Ok(line) => {
                        let waiting = line.contains("heavy load: waiting");
                        lines.push(line);
                        if waiting {
                            break true;
                        }
                    }
                    Err(_) => break false,
                }
            };
            if !ready {
                let _ = child.kill();
                let _ = child.wait();
                reader.join().unwrap();
                panic!(
                    "{command} with removed override={override_value:?} never reached load wait: {lines:?}"
                );
            }
            kill(Pid::from_raw(child.id() as i32), signal).unwrap();
            let cancelled = Instant::now();
            let result = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break Some(status);
                }
                if cancelled.elapsed() > Duration::from_secs(5) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            reader.join().unwrap();
            assert!(
                result.is_some(),
                "{signal}: load wait ignored cancellation for 5s"
            );
            assert!(!result.unwrap().success());
            let queue = root.path().join(".git/aoe-ship/heavy-slots");
            assert_eq!(
                fs::read_dir(queue).unwrap().count(),
                0,
                "cancellation released the ticket cleanly"
            );
            if command == "gate-run" {
                assert_eq!(ledger(output.path())["overall"], "INCOMPLETE");
            }
            assert!(!marker.exists(), "the direct slot command never started");
            assert!(!output.path().join("alpha.log").exists(), "no gate started");
            writeln!(std::io::stdout(),
            "{command} removed override={override_value:?}: {}\n{command} real load wait: {signal} -> non-zero exit, ticket removed, no gate started ({}ms)",
            lines.last().unwrap(), cancelled.elapsed().as_millis()
        ).unwrap();
        }
    }
}
