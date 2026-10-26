//! The CPU cap in a checkout that has no `target/` (a gate's snapshot): make takes the count from
//! the environment the slot holder set and never leaves an empty `--cpus=` or `CARGO_BUILD_JOBS=`.
use std::{fs, process::Command};

fn snapshot() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("slot-cap-")
        .tempdir()
        .unwrap();
    fs::write(
        dir.path().join("Makefile"),
        include_str!("../../../Makefile"),
    )
    .unwrap();
    fs::create_dir(dir.path().join("make")).unwrap();
    for (name, text) in [
        ("make/agents.mk", include_str!("../../../make/agents.mk")),
        ("make/ship.mk", include_str!("../../../make/ship.mk")),
        (
            "make/qualification.mk",
            include_str!("../../../make/qualification.mk"),
        ),
        ("make/heavy.mk", include_str!("../../../make/heavy.mk")),
        ("make/phony.mk", include_str!("../../../make/phony.mk")),
        ("make/help.mk", include_str!("../../../make/help.mk")),
        ("make/assets.mk", include_str!("../../../make/assets.mk")),
        ("make/geodata.mk", include_str!("../../../make/geodata.mk")),
        ("make/release.mk", include_str!("../../../make/release.mk")),
        ("make/sdk.mk", include_str!("../../../make/sdk.mk")),
    ] {
        fs::write(dir.path().join(name), text).unwrap();
    }
    assert!(!dir.path().join("target").exists());
    dir
}

fn dry_run(dir: &tempfile::TempDir, target: &str, slot: Option<(&str, Option<&str>)>) -> String {
    let mut command = Command::new("make");
    command
        .current_dir(dir.path())
        .args(["--dry-run", target])
        .env_remove("AOE_HEAVY_SLOT")
        .env_remove("AOE_SLOT_CPUS");
    for name in ["MAKEFLAGS", "MFLAGS", "GNUMAKEFLAGS"] {
        command.env_remove(name);
    }
    if let Some((token, cpus)) = slot {
        command.env("AOE_HEAVY_SLOT", token);
        if let Some(cpus) = cpus {
            command.env("AOE_SLOT_CPUS", cpus);
        }
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn assert_no_empty_flag(text: &str) {
    for word in text.split_whitespace() {
        for flag in ["--cpus=", "CARGO_BUILD_JOBS="] {
            assert!(word != flag, "empty {flag} in:\n{text}");
        }
    }
}

const TARGETS: &[&str] = &[
    "test-unit",
    "test-unit-steps",
    "build-wasm",
    "build-wasm-steps",
    "fuzz-smoke",
    "fuzz-smoke-steps",
    "map-generate",
    "map-generate-steps",
];

#[test]
fn a_held_slot_caps_every_container_without_a_target_directory() {
    let dir = snapshot();
    for target in TARGETS {
        let text = dry_run(&dir, target, Some(("ticket:nonce", Some("6"))));
        assert_no_empty_flag(&text);
        if target.ends_with("-steps") {
            assert!(text.contains("--cpus=6"), "{target}:\n{text}");
            assert!(text.contains("CARGO_BUILD_JOBS=6"), "{target}:\n{text}");
        }
    }
}

#[test]
fn no_slot_or_no_count_means_no_cap_and_never_an_empty_flag() {
    let dir = snapshot();
    for target in TARGETS {
        for slot in [None, Some(("ticket:nonce", None)), Some(("", Some("6")))] {
            let text = dry_run(&dir, target, slot);
            assert_no_empty_flag(&text);
            assert!(!text.contains("--cpus=6"), "{target} {slot:?}:\n{text}");
        }
    }
}
