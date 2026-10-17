use super::Health;
use crate::devserver::process::{free_port, health_build, http_get, running, spawn, stop};
use std::{os::unix::fs::PermissionsExt, thread, time::Duration};

#[test]
fn health_reports_the_build_and_silence_reports_nothing() {
    let server = Health::serving("abc123", "[]");
    assert_eq!(health_build(server.port).as_deref(), Some("abc123"));
    assert!(http_get(server.port, "/maps/jobs").is_some());
    drop(server);
    thread::sleep(Duration::from_millis(100));
    let silent = free_port(40_000, &[]).expect("port");
    assert_eq!(health_build(silent), None);
}

#[test]
fn a_started_process_is_found_by_its_binary_and_stopped() {
    let directory = tempfile::tempdir().expect("directory");
    let binary = directory.path().join("aoe-server.built");
    std::fs::copy("/bin/sleep", &binary).expect("copy sleep");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let log = directory.path().join("server.log");
    let pid = spawn(&binary, &["60"], directory.path(), &[], &log).expect("spawn");
    for _ in 0..40 {
        if running(pid, &binary) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    assert!(
        running(pid, &binary),
        "pid {pid} cmdline {:?}",
        String::from_utf8_lossy(&cmdline)
    );
    assert!(!running(pid, &directory.path().join("another-binary")));
    stop(pid, &binary);
    // The test is the parent, so the stopped process stays a zombie until reaped.
    let _ = std::process::Command::new("true").status();
    for _ in 0..50 {
        if !running(pid, &binary) {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("the process was not stopped");
}

#[test]
fn free_port_skips_taken_ports() {
    let first = free_port(41_000, &[]).expect("a port");
    let second = free_port(41_000, &[first]).expect("another port");
    assert_ne!(first, second);
}

#[test]
fn stopping_a_process_that_is_gone_is_fine() {
    stop(4_000_000, std::path::Path::new("/nonexistent"));
}
