use super::{Run, container_name, docker_args};
use crate::e2e::shard::Shard;

fn args(specs: &[String], shard: Option<Shard>) -> Vec<String> {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("browser/node_modules")).expect("modules");
    let run = Run {
        root: root.path(),
        address: "127.0.0.1:4000".parse().expect("address"),
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
fn a_shard_reaches_playwright_after_the_npm_separator() {
    let shard = Some(Shard { index: 2, count: 3 });
    assert_eq!(
        tail(&args(&[], shard), 5),
        ["npm", "run", "test:e2e", "--", "--shard=2/3"]
    );
}

#[test]
fn the_whole_suite_and_focused_runs_carry_no_shard() {
    assert_eq!(tail(&args(&[], None), 3), ["npm", "run", "test:e2e"]);
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
            "--reporter=json",
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
