use super::*;
use nix::time::{ClockId, clock_gettime};
use std::io::Write;

fn real_gap() -> i64 {
    clock_gettime(ClockId::CLOCK_BOOTTIME).unwrap().tv_sec()
        - clock_gettime(ClockId::CLOCK_MONOTONIC).unwrap().tv_sec()
}

#[test]
fn production_capture_ignores_the_simulated_gap_file() {
    let root = fixture();
    let output = tempfile::tempdir().unwrap();
    let gap = output.path().join("gap");
    fs::write(&gap, "300").unwrap();
    fs::write(
        root.path().join("Makefile"),
        "alpha:\n\t@printf 'BOOT_GAP=%s\\n' \"$$AOE_HARNESS_BOOT_GAP\"\nbeta:\n\t@true\n",
    )
    .unwrap();
    let before = real_gap();
    let result = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root.path())
        .env("AOE_HARNESS_SIMULATED_GAP_FILE", &gap)
        .args([
            "gate-run",
            "--job",
            "core",
            "--output",
            output.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = fs::read_to_string(output.path().join("alpha.log")).unwrap();
    let actual: i64 = log
        .lines()
        .find_map(|line| line.strip_prefix("BOOT_GAP="))
        .expect("child gap in retained stdout")
        .parse()
        .unwrap();
    let after = real_gap();
    assert!(
        (before.min(after) - 1..=before.max(after) + 1).contains(&actual),
        "production gap {actual}s includes the synthetic 300s; real gap was {before}..{after}s"
    );
    writeln!(std::io::stdout(), "F6: production gate-run with simulated gap file=300: PASS; child gap {actual}s, real clock {before}..{after}s").unwrap();
}
