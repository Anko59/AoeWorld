//! Exercise the real launcher and Make recipe with a small stand-in for Docker.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

fn run(root: &Path, args: &[&str]) {
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

/// Stands in for Docker: "builds" by copying the stand-in judge, and runs the
/// judge for any other container command.
const DOCKER: &str = r#"#!/bin/sh
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

/// Fails unless it runs inside the launcher's build lock, after the build.
const LOCKED_JUDGE: &str = r#"#!/bin/sh
if flock -n "$DISK_TEST_HOME/build.lock" true; then exit 44; fi
[ ! -d "$DISK_TEST_HOME/build-active" ] || exit 45
echo "$*" >> "$DISK_TEST_HOME/observed"
"#;

/// Only records how it was called.
const RECORDING_JUDGE: &str = r#"#!/bin/sh
echo "$*" >> "$DISK_TEST_HOME/observed"
"#;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A repository holding only the real launcher, its Make recipe and the probe
/// file, with `origin/dev` at HEAD and the stand-ins in `bin/`.
fn fixture(judge: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, &["init", "-q", "-b", "dev"]);
    for file in [".agents/hooks/harness.sh", "make/agents.mk"] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::copy(repository().join(file), root.join(file)).unwrap();
    }
    fs::create_dir_all(root.join("crates/harness/src/agents")).unwrap();
    fs::write(root.join("crates/harness/src/agents/runtime.rs"), "").unwrap();
    fs::write(
        root.join("Makefile"),
        "ROOT := $(CURDIR)\nGIT_COMMON := $(ROOT)/.git\nUID := 0\nGID := 0\nTOOL_IMAGE := fixture\ninclude make/agents.mk\n",
    )
    .unwrap();
    advance_dev(root, "fixture");
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    for (name, script) in [("docker", DOCKER), ("judge", judge)] {
        fs::write(bin.join(name), script).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    temp
}

/// Commits the working tree and moves `origin/dev` to it.
fn advance_dev(root: &Path, message: &str) {
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "-m", message]);
    run(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
}

fn launch(root: &Path, arguments: &[&str]) -> Command {
    let bin = root.join("bin");
    let mut command = Command::new("sh");
    command
        .arg(root.join(".agents/hooks/harness.sh"))
        .args(arguments)
        .current_dir(root)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("DISK_TEST_HOME", root.join(".git/aoe-agent-hook"))
        .env("DISK_TEST_BIN", &bin)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    command
}

#[test]
fn disk_hygiene_hook_build_and_cleanup_hold_the_same_lock() {
    let temp = fixture(LOCKED_JUDGE);
    let root = temp.path();
    let home = root.join(".git/aoe-agent-hook");
    let mut first = launch(root, &["build"]).spawn().unwrap();
    let mut second = launch(root, &["build"]).spawn().unwrap();
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

fn observed(root: &Path, line: &str) -> usize {
    fs::read_to_string(root.join(".git/aoe-agent-hook/observed"))
        .unwrap()
        .lines()
        .filter(|observed| *observed == line)
        .count()
}

#[test]
fn judge_is_rebuilt_only_when_its_compiled_inputs_change() {
    let temp = fixture(RECORDING_JUDGE);
    let root = temp.path();
    let hook = || {
        assert!(
            launch(root, &["claude", "stop"])
                .status()
                .unwrap()
                .success()
        )
    };
    hook();
    assert_eq!(observed(root, "build"), 1);

    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/note.md"), "words\n").unwrap();
    advance_dev(root, "docs only");
    hook();
    assert_eq!(
        observed(root, "build"),
        1,
        "a docs-only dev commit reuses the judge"
    );

    fs::write(
        root.join("crates/harness/src/agents/runtime.rs"),
        "// changed\n",
    )
    .unwrap();
    advance_dev(root, "code");
    hook();
    assert_eq!(
        observed(root, "build"),
        2,
        "changed code builds a new judge"
    );

    fs::create_dir_all(root.join("gates")).unwrap();
    fs::write(root.join("gates/container.args"), "--memory=4g\n").unwrap();
    advance_dev(root, "embedded gate file");
    hook();
    assert_eq!(
        observed(root, "build"),
        3,
        "a changed embedded file builds a new judge"
    );
    assert_eq!(observed(root, "agent-hook --runtime claude stop"), 4);
}

/// The paths the launcher hashes into the judge's key.
const KEY_INPUTS: [&str; 6] = [
    "crates",
    "gates",
    "fuzz",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

fn production_sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(name.as_str(), "tests" | "benches" | "examples" | "target") {
                production_sources(&path, found);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            found.push(path);
        }
    }
}

#[test]
fn judge_key_covers_every_file_compiled_into_the_workspace() {
    let repo = repository().canonicalize().unwrap();
    let launcher = fs::read_to_string(repo.join(".agents/hooks/harness.sh")).unwrap();
    let listing = format!("ls-tree \"$revision\" -- {}", KEY_INPUTS.join(" "));
    assert!(
        launcher.contains(&listing),
        "the launcher must hash {listing}"
    );

    let mut sources = Vec::new();
    production_sources(&repo.join("crates"), &mut sources);
    let mut outside = Vec::new();
    for source in sources {
        let text = fs::read_to_string(&source).unwrap();
        for marker in ["include_str!(\"", "include_bytes!(\""] {
            for (at, _) in text.match_indices(marker) {
                let literal = &text[at + marker.len()..];
                let literal = &literal[..literal.find('"').unwrap()];
                let embedded = source
                    .parent()
                    .unwrap()
                    .join(literal)
                    .canonicalize()
                    .unwrap();
                let relative = embedded.strip_prefix(&repo).unwrap().to_path_buf();
                if !KEY_INPUTS.iter().any(|input| relative.starts_with(input)) {
                    outside.push((source.clone(), relative));
                }
            }
        }
    }
    assert!(
        outside.is_empty(),
        "embedded files outside the judge key (add their directory to the launcher and KEY_INPUTS): {outside:?}"
    );
}
