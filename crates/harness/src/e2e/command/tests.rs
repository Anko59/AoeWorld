use super::{Run, container_name, docker_args};
use crate::e2e::shard::Shard;

fn args(specs: &[String], shard: Option<Shard>) -> Vec<String> {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("browser/node_modules")).expect("modules");
    let run = Run {
        root: root.path(),
        base_urls: "http://127.0.0.1:4000".into(),
        container: "name".into(),
        landscape_hash: "hash",
        specs,
        shard,
    };
    docker_args(&run).expect("arguments")
}

fn tail(args: &[String], count: usize) -> Vec<&str> {
    args[args.len() - count..]
        .iter()
        .map(String::as_str)
        .collect()
}

#[test]
fn the_container_carries_the_boot_gap_it_started_at() {
    let args = args(&[], None);
    let label = args
        .windows(2)
        .find(|pair| pair[0] == "--label")
        .map(|pair| pair[1].as_str())
        .expect("a label");
    assert!(label.starts_with("aoe.harness-boot-gap="), "{label}");
}

#[test]
fn every_option_keeps_its_own_value_and_the_image_follows_them() {
    let args = args(&[], None);
    for (index, arg) in args.iter().enumerate() {
        if arg == "-e" {
            let value = args.get(index + 1).map_or("", String::as_str);
            assert!(
                value.contains('='),
                "-e takes NAME=value, not {value:?}: {args:?}"
            );
        }
    }
    let image = args
        .iter()
        .position(|arg| arg == "xvfb-run")
        .expect("the command");
    assert_eq!(args[image - 1], "aoeworld/browser-tools:1.64.0", "{args:?}");
    assert_eq!(args[image - 3], "-w", "{args:?}");
}

#[test]
fn a_shard_reaches_playwright_after_the_npm_separator() {
    let shard = Some(Shard { index: 2, count: 3 });
    assert_eq!(
        tail(&args(&[], shard), 7),
        [
            "npm",
            "--silent",
            "run",
            "test:e2e",
            "--",
            "--reporter=./quarantine-reporter.ts,json",
            "--shard=2/3"
        ]
    );
}

#[test]
fn the_whole_suite_and_focused_runs_carry_no_shard() {
    assert_eq!(
        tail(&args(&[], None), 6),
        [
            "npm",
            "--silent",
            "run",
            "test:e2e",
            "--",
            "--reporter=./quarantine-reporter.ts,json"
        ]
    );
    let focused = args(
        &["tests/a.spec.ts".into()],
        Some(Shard { index: 1, count: 3 }),
    );
    assert!(focused.iter().all(|arg| !arg.starts_with("--shard")));
    assert_eq!(
        tail(&focused, 7),
        [
            "npm",
            "--silent",
            "run",
            "test:e2e",
            "--",
            "--reporter=./quarantine-reporter.ts,json",
            "tests/a.spec.ts"
        ]
    );
}

#[test]
fn a_shard_names_its_container() {
    assert_eq!(container_name(7, 9, None), "aoeworld-e2e-7-9");
    assert_eq!(
        container_name(7, 9, Some(Shard { index: 2, count: 3 })),
        "aoeworld-e2e-shard-2-of-3-7-9"
    );
}

#[test]
fn the_container_gets_every_server_url_and_no_single_base_url() {
    let args = args(&[], None);
    assert!(args.contains(&"AOE_BASE_URLS=http://127.0.0.1:4000".to_owned()));
    assert!(args.iter().all(|arg| !arg.starts_with("AOE_BASE_URL=")));
}

#[test]
fn a_filtered_run_passes_its_resolved_specs_after_the_json_reporter() {
    let specs = ["tests/village/selection-density.play.spec.ts".to_owned()];
    assert_eq!(
        tail(&args(&specs, None), 7),
        [
            "npm",
            "--silent",
            "run",
            "test:e2e",
            "--",
            "--reporter=./quarantine-reporter.ts,json",
            "tests/village/selection-density.play.spec.ts"
        ]
    );
}
