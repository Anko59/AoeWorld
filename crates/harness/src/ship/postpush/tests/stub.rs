//! A stubbed `gh` first on PATH answering from fixture files, and the ship
//! fixture whose `origin` URL is github.com/project/checkout but whose objects
//! live in a local bare repository, under the issue tests' environment lock.
use crate::ship::{
    git,
    issues::tests::ENVIRONMENT,
    tests::{fixture, run},
};
use std::{ffi::OsString, fs, os::unix::fs::PermissionsExt, path::PathBuf, sync::MutexGuard};

const GH: &str = r#"#!/bin/sh
printf -- '---\n' >> "$GH_LOG"
printf '<%s>\n' "$@" >> "$GH_LOG"
if [ "$1 $2" = "pr view" ]; then
  case "$*" in
    *headRefOid*) cat "$GH_DIR/review-meta.json" ;;
    *) echo '{"body":"Dependency update"}' ;;
  esac
  exit 0
fi
[ "$1" = api ] || exit 0
all="$*"
case "$all" in
  *"--method POST"*|*"--method PATCH"*)
    case "$4" in
      */dispatches*) [ -f "$GH_DIR/dispatch-fails" ] && { echo "dispatch denied" >&2; exit 1; } ;;
      */statuses/*) [ -f "$GH_DIR/carry-fails" ] && { echo "status denied" >&2; exit 1; } ;;
    esac
    [ -f "$GH_DIR/write-fails" ] && { echo "stub failure" >&2; exit 1; }
    echo '{}' ;;
  *UpdateBranch*)
    id=$(printf '%s\n' "$@" | sed -n 's/^id=//p')
    if [ -f "$GH_DIR/rebase-fails-$id" ]; then cat "$GH_DIR/rebase-fails-$id" >&2; exit 1; fi
    echo '{"data":{}}' ;;
  *ListWaiting*) cat "$GH_DIR/list.json" ;;
  *UpdatedHead*)
    n=$(printf '%s\n' "$@" | sed -n 's/^number=//p')
    if [ -f "$GH_DIR/poll-old-$n.json" ]; then
      cat "$GH_DIR/poll-old-$n.json"
      rm "$GH_DIR/poll-old-$n.json"
    else cat "$GH_DIR/head-$n.json"; fi ;;
  */compare/*)
    oid=$(printf '%s\n' "$@" | sed -n 's|.*/compare/dev\.\.\.||p')
    cat "$GH_DIR/compare-$oid.json" ;;
  */pulls/*)
    n=$(printf '%s\n' "$@" | sed -n 's|.*/pulls/\([0-9]*\)$|\1|p')
    cat "$GH_DIR/pull-$n.json" ;;
  */comments*)
    n=$(printf '%s\n' "$@" | sed -n 's|.*/issues/\([0-9]*\)/comments.*|\1|p')
    if [ -f "$GH_DIR/comments-$n.json" ]; then cat "$GH_DIR/comments-$n.json"; else echo '[]'; fi ;;
esac
"#;

/// `git@github.com:project/checkout.git` is served from the local bare repository.
const SSH: &str = r#"#!/bin/sh
# Git passes options (`-o SendEnv=GIT_PROTOCOL`), the host, then the command.
while [ "${1#-}" != "$1" ]; do shift 2; done
shift
eval "set -- $1"
case "$1" in
  git-upload-pack) exec git upload-pack "$GIT_STUB_ORIGIN" ;;
  git-receive-pack) exec git receive-pack "$GIT_STUB_ORIGIN" ;;
esac
exit 1
"#;

pub(super) struct World {
    _guard: MutexGuard<'static, ()>,
    pub(super) temp: tempfile::TempDir,
    pub(super) root: PathBuf,
    dir: PathBuf,
    log: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl World {
    pub(super) fn new() -> Self {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let (temp, root) = fixture("true");
        run(&root, &["config", "user.name", "t"]);
        run(&root, &["config", "user.email", "t@example.com"]);
        let origin = temp.path().join("origin.git");
        run(
            &root,
            &[
                "remote",
                "set-url",
                "origin",
                "git@github.com:project/checkout.git",
            ],
        );
        let bin = temp.path().join("bin");
        let dir = temp.path().join("gh-answers");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&dir).unwrap();
        for (name, script) in [("gh", GH), ("ssh", SSH)] {
            fs::write(bin.join(name), script).unwrap();
            fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let log = temp.path().join("gh.log");
        fs::write(&log, "").unwrap();
        let mut world = Self {
            _guard: guard,
            temp,
            root,
            dir,
            log,
            saved: Vec::new(),
        };
        let path = std::env::var_os("PATH").unwrap_or_default();
        world.set(
            "PATH",
            &format!("{}:{}", bin.display(), path.to_string_lossy()),
        );
        world.set("GIT_SSH_COMMAND", &bin.join("ssh").display().to_string());
        world.set("GIT_STUB_ORIGIN", &origin.display().to_string());
        world.set("GH_LOG", &world.log.display().to_string());
        world.set("GH_DIR", &world.dir.display().to_string());
        for name in [
            "AOE_JUDGE_REV",
            "SHIP_BASE",
            "GITHUB_REPOSITORY",
            "GH_REPO",
            "AOE_AGENT_ROLE",
            "GITHUB_ACTIONS",
            "GH_TOKEN",
        ] {
            world.unset(name);
        }
        world
    }

    pub(super) fn set(&mut self, name: &'static str, value: &str) {
        self.saved.push((name, std::env::var_os(name)));
        unsafe { std::env::set_var(name, value) };
    }

    pub(super) fn unset(&mut self, name: &'static str) {
        self.saved.push((name, std::env::var_os(name)));
        unsafe { std::env::remove_var(name) };
    }

    /// Makes `gh api` answer `name` with `content`.
    pub(super) fn answer(&self, name: &str, content: &str) {
        fs::write(self.dir.join(name), content).unwrap();
    }

    /// Every `gh` call so far, one `<arg>` per line, calls split by `---`.
    pub(super) fn calls(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    pub(super) fn clear(&self) {
        fs::write(&self.log, "").unwrap();
    }

    /// Runs the branch's compiled CLI, with this world's Git and API fixtures.
    pub(super) fn cli(&self, command: &str) -> std::process::Output {
        let binary = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("aoe-harness");
        let output = std::process::Command::new(binary)
            .current_dir(&self.root)
            .arg(command)
            .output()
            .unwrap();
        use std::io::Write;
        let proof = format!(
            "$ aoe-harness {command}\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::io::stdout().write_all(proof.as_bytes()).unwrap();
        output
    }

    pub(super) fn script(&self, name: &str, body: &str) {
        let path = self.temp.path().join("bin").join(name);
        fs::write(&path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub(super) fn head(&self) -> String {
        git::git(&self.root, &["rev-parse", "HEAD"]).unwrap()
    }

    /// Publishes `sha` as the head of pull request `number` on origin.
    pub(super) fn publish_pull(&self, number: u64, sha: &str) {
        run(
            &self.root,
            &[
                "push",
                "-q",
                "-f",
                "origin",
                &format!("{sha}:refs/pull/{number}/head"),
            ],
        );
    }

    /// Moves origin's `dev` by one commit that writes `file`; returns it.
    pub(super) fn move_dev(&self, file: &str, text: &str) -> String {
        let other = self.temp.path().join("other");
        if !other.exists() {
            run(
                self.temp.path(),
                &[
                    "clone",
                    "-q",
                    self.origin().to_str().unwrap(),
                    other.to_str().unwrap(),
                ],
            );
            run(&other, &["config", "user.name", "t"]);
            run(&other, &["config", "user.email", "t@example.com"]);
        }
        run(&other, &["pull", "-q", "origin", "dev"]);
        fs::create_dir_all(other.join(file).parent().unwrap()).unwrap();
        fs::write(other.join(file), text).unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", &format!("dev: {file}")]);
        run(&other, &["push", "-q", "origin", "dev"]);
        git::git(&other, &["rev-parse", "HEAD"]).unwrap()
    }

    pub(super) fn origin(&self) -> PathBuf {
        self.temp.path().join("origin.git")
    }

    /// The feature branch replayed onto origin's current `dev`; returns the
    /// new head.
    pub(super) fn rebase_onto_dev(&self) -> String {
        run(&self.root, &["fetch", "-q", "origin", "dev"]);
        run(&self.root, &["rebase", "-q", "FETCH_HEAD"]);
        self.head()
    }

    /// Origin's current `dev` merged into the feature branch, as GitHub's
    /// MERGE update does; returns the new head.
    pub(super) fn merge_dev(&self) -> String {
        run(&self.root, &["fetch", "-q", "origin", "dev"]);
        run(&self.root, &["merge", "-q", "--no-edit", "FETCH_HEAD"]);
        self.head()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        for (name, value) in self.saved.drain(..).rev() {
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

pub(super) fn pull_json(state: &str, base: &str, head: &str) -> String {
    serde_json::json!({"state": state, "base": {"ref": base}, "head": {"sha": head}}).to_string()
}

pub(super) fn comment(id: u64, association: &str, body: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "author_association": association, "body": body})
}
