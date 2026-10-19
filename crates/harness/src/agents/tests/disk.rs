//! Exercise the real launcher and Make recipe with a small stand-in for Docker.
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

fn run(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("PATH", "/usr/bin:/bin")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn disk_hygiene_hook_build_and_cleanup_hold_the_same_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, &["init", "-q", "-b", "dev"]);
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for file in [".agents/hooks/harness.sh", "make/agents.mk"] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::copy(repo.join(file), root.join(file)).unwrap();
    }
    fs::create_dir_all(root.join("crates/harness/src/agents")).unwrap();
    fs::write(root.join("crates/harness/src/agents/runtime.rs"), "").unwrap();
    fs::write(
        root.join("Makefile"),
        "ROOT := $(CURDIR)\nGIT_COMMON := $(ROOT)/.git\nUID := 0\nGID := 0\nTOOL_IMAGE := fixture\ninclude make/agents.mk\n",
    )
    .unwrap();
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "-m", "fixture"]);
    run(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let home = root.join(".git/aoe-agent-hook");
    let docker = r#"#!/bin/sh
case "$*" in
  *'cargo build '*)
    if flock -n "$DISK_TEST_HOME/build.lock" true; then exit 41; fi
    mkdir "$DISK_TEST_HOME/build-active" || exit 42
    sleep 0.1
    mkdir -p "$DISK_TEST_HOME/target/debug"
    cp "$DISK_TEST_BIN/judge" "$DISK_TEST_HOME/target/debug/aoe-harness"
    rmdir "$DISK_TEST_HOME/build-active"
    echo build >> "$DISK_TEST_HOME/observed"
    exit 0 ;;
esac
while [ "$#" -gt 0 ]; do
  case "$1" in */aoe-harness) exec "$@" ;; esac
  shift
done
exit 43
"#;
    let judge = r#"#!/bin/sh
if flock -n "$DISK_TEST_HOME/build.lock" true; then exit 44; fi
[ ! -d "$DISK_TEST_HOME/build-active" ] || exit 45
echo "$*" >> "$DISK_TEST_HOME/observed"
"#;
    for (name, script) in [("docker", docker), ("judge", judge)] {
        fs::write(bin.join(name), script).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let launch = || {
        let mut command = Command::new("sh");
        command
            .arg(root.join(".agents/hooks/harness.sh"))
            .arg("build")
            .current_dir(root)
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("DISK_TEST_HOME", &home)
            .env("DISK_TEST_BIN", &bin)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
        command
    };
    let mut first = launch().spawn().unwrap();
    let mut second = launch().spawn().unwrap();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    let observed = fs::read_to_string(home.join("observed")).unwrap();
    assert_eq!(observed.lines().filter(|line| *line == "build").count(), 2);
    assert_eq!(
        observed
            .lines()
            .filter(|line| line.starts_with("agent-prune "))
            .count(),
        2
    );
}
