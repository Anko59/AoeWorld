//! The real launchers as a Mac runs them, with stand-ins for `uname`, `rustup`,
//! the VM's `ssh` and OrbStack's `orb`, on any system the tests run on.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

/// Every command a Mac runs itself, in the order the launcher lists them.
const NATIVE: &str = "ship ship-status describe-check review review-floor review-pr issue next carry-review update-prs";

const UNAME: &str = r#"case "$1" in -m) echo arm64 ;; *) echo "$MAC_OS" ;; esac"#;

/// Pinned toolchain: records each build and "compiles" the stand-in judge.
const RUSTUP: &str = r#"[ ! -e "$HOME/no-toolchain" ] || exit 1
[ "$1" = run ] || exit 2
[ "$3" = cargo ] || { shift 2; exec "$@"; }
[ "$4" != --version ] || exit 0
echo "$2 in $(pwd): $*" >>"$HOME/builds"
while [ $# -gt 0 ]; do
  case $1 in --manifest-path) source=$(dirname "$2") ;; --target-dir) target=$2 ;; esac
  shift
done
cat "$source/crates/harness/src/ship/mod.rs" >>"$HOME/built-from"
sleep 0.2
mkdir -p "$target/debug" && cp "$HOME/judge" "$target/debug/aoe-harness""#;

const JUDGE: &str = r#"#!/bin/sh
[ "$1" != agent-prune ] || exit 0
{ echo "judge $*"; echo "rev ${AOE_JUDGE_REV-}"; echo "tmp ${TMPDIR-}"; echo "title ${SHIP_TITLE-}"; cat; } >>"$HOME/judged"
exit 3
"#;

/// Linux `make agent-hook-build`, and the only command Linux builds with.
const MAKE: &str = r#"for argument; do case $argument in AGENT_HOOK_OUTPUT=*) output=${argument#*=} ;; esac; done
echo "$*" >>"$HOME/make"
mkdir -p "$(dirname "$output")" && cp "$HOME/judge" "$output""#;

struct Mac {
    _temp: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
}

fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Mac {
    /// A home with a Colima VM and a checkout whose `origin/dev` is HEAD.
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap();
        let root = home.join("repo");
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for file in [
            ".agents/hooks/harness.sh",
            ".agents/hooks/colima.sh",
            ".agents/hooks/mac-judge.sh",
            "GNUmakefile",
            "make/ship.mk",
        ] {
            fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            fs::copy(repository.join(file), root.join(file)).unwrap();
        }
        let mac = Self {
            _temp: temp,
            home,
            root,
        };
        mac.file("crates/harness/src/ship/mod.rs", "dev\n");
        mac.file("crates/harness/src/ship/issues.rs", "");
        mac.file("rust-toolchain.toml", "[toolchain]\nchannel = \"1.2.3\"\n");
        fs::create_dir_all(mac.home.join("bin")).unwrap();
        for (name, body) in [
            ("uname", UNAME),
            ("rustup", RUSTUP),
            ("ssh", "echo \"$*\" >>\"$HOME/vm\""),
            (
                "orb",
                "[ \"$1\" != list ] || { echo aoeworld; exit 0; }\necho \"$*\" >>\"$HOME/orb\"",
            ),
        ] {
            script(
                &mac.home.join("bin").join(name),
                &format!("#!/bin/sh\n{body}\n"),
            );
        }
        script(&mac.home.join("judge"), JUDGE);
        let vm = mac.home.join(".colima/_lima/colima");
        fs::create_dir_all(&vm).unwrap();
        fs::write(vm.join("ssh.config"), "").unwrap();
        mac.git(&["init", "-q", "-b", "dev"]);
        mac.advance_dev();
        mac
    }

    fn file(&self, path: &str, text: &str) {
        fs::create_dir_all(self.root.join(path).parent().unwrap()).unwrap();
        fs::write(self.root.join(path), text).unwrap();
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = self
            .command("git", &["-c", "user.name=t", "-c", "user.email=t@e"])
            .args(["-c", "commit.gpgsign=false"])
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {arguments:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn advance_dev(&self) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "dev"]);
        self.git(&["update-ref", "refs/remotes/origin/dev", "HEAD"]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Only what a test names reaches the launchers.
    fn command(&self, program: &str, arguments: &[&str]) -> Command {
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(&self.root)
            .env_clear()
            .env("HOME", &self.home)
            .env("MAC_OS", "Darwin")
            .env("TMPDIR", "/var/folders/x")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("PATH", self.path())
            .stdin(Stdio::null());
        command
    }

    fn path(&self) -> String {
        format!("{}:/usr/bin:/bin", self.home.join("bin").display())
    }

    fn launch(&self, arguments: &[&str]) -> Command {
        let launcher = self.root.join(".agents/hooks/harness.sh");
        let mut command = self.command(launcher.to_str().unwrap(), arguments);
        command.env("AOE_NATIVE_SHIP", "1");
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.launch(arguments).output().unwrap()
    }

    fn read(&self, file: &str) -> String {
        fs::read_to_string(self.home.join(file)).unwrap_or_default()
    }

    fn key(&self) -> String {
        let inputs = "crates gates fuzz Cargo.toml Cargo.lock rust-toolchain.toml";
        let line = format!("git ls-tree origin/dev -- {inputs} | git hash-object --stdin");
        let output = self.command("sh", &["-c", &line]).output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn judges(&self) -> PathBuf {
        self.root.join(".git/aoe-agent-hook")
    }
}

#[test]
fn a_mac_builds_the_ship_judge_from_origin_dev_apart_from_the_linux_judge() {
    let mac = Mac::new();
    let dev = mac.git(&["rev-parse", "origin/dev"]);
    mac.file("crates/harness/src/ship/mod.rs", "branch\n");
    mac.git(&["commit", "-qam", "branch"]);
    mac.file("crates/harness/src/ship/mod.rs", "tree\n");
    mac.file(".cargo/config.toml", "[build]\nrustc-wrapper = \"evil\"\n");
    let output = mac.run(&["exec", "ship"]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "the judge's own status: {output:?}"
    );
    assert_eq!(mac.read("built-from"), "dev\n");
    let builds = mac.read("builds");
    // From `/`, so no directory of the checkout configures Cargo.
    assert!(builds.starts_with("1.2.3 in /: "), "{builds}");
    assert!(
        builds.contains("cargo build --locked -p aoe-harness "),
        "{builds}"
    );
    let judge = mac.judges().join("darwin-arm64").join(mac.key());
    assert!(judge.join("aoe-harness").is_file());
    assert!(
        !mac.judges().join(mac.key()).exists(),
        "never the Linux name"
    );
    let judged = mac.read("judged");
    assert!(
        judged.starts_with(&format!("judge ship\nrev {dev}\n")),
        "{judged}"
    );
    // Work trees the judge hands to the VM must be under the mounted home.
    let temporary = format!("tmp {}/", mac.home.display());
    assert!(judged.contains(&temporary), "{judged}");
    assert_eq!(mac.read("vm"), "");
}

#[test]
fn the_mac_judge_is_built_once_per_set_of_compiled_inputs() {
    let mac = Mac::new();
    let mut both = [(); 2].map(|()| mac.launch(&["exec", "ship"]).spawn().unwrap());
    for child in &mut both {
        assert_eq!(child.wait().unwrap().code(), Some(3));
    }
    assert_eq!(
        mac.read("builds").lines().count(),
        1,
        "one builder at a time"
    );
    mac.file("docs/note.md", "words\n");
    mac.advance_dev();
    mac.run(&["exec", "ship"]);
    assert_eq!(mac.read("builds").lines().count(), 1);
    mac.file("gates/container.args", "--memory=4g\n");
    mac.advance_dev();
    mac.run(&["exec", "ship"]);
    assert_eq!(mac.read("builds").lines().count(), 2);
}

#[test]
fn without_the_pinned_toolchain_one_line_names_the_docs_and_orbstack() {
    let mac = Mac::new();
    fs::write(mac.home.join("no-toolchain"), "").unwrap();
    let output = mac.run(&["exec", "ship"]);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert_eq!(error.lines().count(), 1, "{error}");
    for part in ["1.2.3", "docs/local-development.md", "OrbStack"] {
        assert!(error.contains(part), "{error}");
    }
    assert_eq!(mac.read("judged") + &mac.read("vm"), "");
}

#[test]
fn a_mac_never_builds_a_judge_from_its_checkout() {
    let mac = Mac::new();
    mac.git(&["update-ref", "-d", "refs/remotes/origin/dev"]);
    let output = mac.run(&["exec", "ship"]);
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("origin/dev"), "{error}");
    assert_eq!(mac.read("builds") + &mac.read("judged"), "");
}

#[test]
fn everything_but_an_opted_in_ship_command_still_runs_in_the_vm() {
    let mac = Mac::new();
    for arguments in [
        &["exec", "duel"][..],
        &["exec", "showcase"],
        &["claude", "pre-tool-use"],
        &["codex", "stop"],
        &["build"],
    ] {
        let before = mac.read("vm").lines().count();
        assert!(mac.run(arguments).status.success(), "{arguments:?}");
        assert_eq!(mac.read("vm").lines().count(), before + 1, "{arguments:?}");
    }
    for value in ["", "0"] {
        let before = mac.read("vm").lines().count();
        let mut command = mac.launch(&["exec", "ship"]);
        assert!(
            command
                .env("AOE_NATIVE_SHIP", value)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(mac.read("vm").lines().count(), before + 1, "{value:?}");
    }
    fs::remove_file(mac.home.join(".colima/_lima/colima/ssh.config")).unwrap();
    assert!(mac.run(&["exec", "ship"]).status.success());
    assert!(
        mac.read("orb").contains("harness.sh"),
        "{}",
        mac.read("orb")
    );
    assert_eq!(mac.read("builds") + &mac.read("judged"), "");
}

#[test]
fn every_native_command_is_one_the_launcher_and_make_know() {
    let mac = Mac::new();
    let judge = mac.root.join(".agents/hooks/mac-judge.sh");
    let goals = mac
        .command(judge.to_str().unwrap(), &["goals"])
        .env("AOE_NATIVE_SHIP", "1")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(goals.stdout).unwrap().trim(), NATIVE);
    let make = fs::read_to_string(mac.root.join("make/ship.mk")).unwrap();
    let launcher = fs::read_to_string(mac.root.join(".agents/hooks/harness.sh")).unwrap();
    for goal in NATIVE.split(' ') {
        assert!(make.contains(&format!(
            "\n{goal}:\n\t@.agents/hooks/harness.sh exec {goal}\n"
        )));
        assert!(launcher.contains(goal));
    }
}

#[test]
fn make_runs_a_native_goal_with_its_variables_and_sends_every_other_goal_to_the_vm() {
    let mac = Mac::new();
    let make = |arguments: &[&str]| mac.command("make", arguments).output().unwrap();
    make(&["ship", "AOE_NATIVE_SHIP=1", "SHIP_TITLE=it's a title"]);
    let judged = mac.read("judged");
    assert!(judged.contains("judge ship\n"), "{judged}");
    assert!(judged.contains("title it's a title\n"), "{judged}");
    let mut issue = mac.command("make", &["issue", "AOE_NATIVE_SHIP=1"]);
    fs::write(mac.home.join("in"), "Standard input\n").unwrap();
    issue.stdin(fs::File::open(mac.home.join("in")).unwrap());
    issue.output().unwrap();
    assert!(mac.read("judged").ends_with("Standard input\n"));
    assert_eq!(mac.read("vm"), "");
    for arguments in [
        &["ship"][..],
        &["lint", "AOE_NATIVE_SHIP=1"],
        &["ship", "lint", "AOE_NATIVE_SHIP=1"],
        &["duel", "AOE_NATIVE_SHIP=1"],
        &["AOE_NATIVE_SHIP=1"],
    ] {
        let before = mac.read("vm").lines().count();
        assert!(make(arguments).status.success(), "{arguments:?}");
        assert_eq!(mac.read("vm").lines().count(), before + 1, "{arguments:?}");
    }
    assert_eq!(mac.read("judged").matches("judge ").count(), 2);
}

#[test]
fn linux_ignores_the_mac_route() {
    let mac = Mac::new();
    for (name, body) in [("make", MAKE), ("flock", "shift; exec \"$@\"")] {
        script(
            &mac.home.join("bin").join(name),
            &format!("#!/bin/sh\n{body}\n"),
        );
    }
    let mut linux = mac.launch(&["exec", "ship"]);
    assert_eq!(
        linux.env("MAC_OS", "Linux").status().unwrap().code(),
        Some(3)
    );
    let binary = mac.judges().join(mac.key()).join("aoe-harness");
    assert!(binary.is_file());
    let built = mac.read("make");
    assert!(
        built.contains(&format!("AGENT_HOOK_OUTPUT={}", binary.display())),
        "{built}"
    );
    assert!(mac.read("judged").contains("tmp /var/folders/x\n"));
    assert!(!mac.judges().join("darwin-arm64").exists());
    assert_eq!(mac.read("builds"), "");
}
