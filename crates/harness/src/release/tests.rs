use super::*;
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct FakeRuntime {
    calls: RefCell<Vec<Vec<String>>>,
    dirty: Cell<bool>,
    wrong_branch: Cell<bool>,
    fail_copy: Cell<bool>,
    remove_bundle_file_on_image_copy: Cell<bool>,
    missing_shader_in: RefCell<Option<String>>,
}

impl Runtime for FakeRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        self.calls.borrow_mut().push(
            std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
        );
        match (program, args) {
            ("git", ["rev-parse", "HEAD"]) => Ok("a".repeat(40)),
            ("git", ["rev-parse", value]) if value.ends_with("^{tree}") => Ok("b".repeat(40)),
            ("git", ["status", "--porcelain", "--untracked-files=all"]) => {
                Ok(if self.dirty.get() { "modified" } else { "" }.to_owned())
            }
            ("git", ["branch", "--show-current"]) => Ok(if self.wrong_branch.get() {
                "feature"
            } else {
                "dev"
            }
            .to_owned()),
            ("docker", ["create", image]) => {
                Ok(if image.contains("/server:") || *image == "sha256:server" {
                    "server-container"
                } else if image.contains("/browser:") || *image == "sha256:browser" {
                    "browser-container"
                } else {
                    "temporary-container"
                }
                .to_owned())
            }
            ("docker", ["image", "inspect", .., tag]) if tag.contains("/server:") => {
                Ok("sha256:server".to_owned())
            }
            ("docker", ["image", "inspect", .., tag]) if tag.contains("/browser:") => {
                Ok("sha256:browser".to_owned())
            }
            ("rustc", ["--version"]) => Ok("rustc 1.99.1".to_owned()),
            _ => Err(format!("unexpected output command: {program} {args:?}").into()),
        }
    }

    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        self.calls.borrow_mut().push(
            std::iter::once(program)
                .chain(args.iter().copied())
                .map(str::to_owned)
                .collect(),
        );
        if program != "docker" {
            return Err("unexpected checked program".into());
        }
        if let ["cp", source, directory] = args {
            if self.fail_copy.get() {
                return Err("injected copy failure".into());
            }
            fs::write(Path::new(directory).join("index.html"), b"release bundle")?;
            if source.starts_with("server-container") && self.remove_bundle_file_on_image_copy.get()
            {
                let calls = self.calls.borrow();
                let artifact = calls
                    .iter()
                    .find(|call| {
                        call.get(2)
                            .is_some_and(|arg| arg.starts_with("temporary-container:"))
                    })
                    .ok_or("missing artifact export")?;
                fs::remove_file(Path::new(&artifact[3]).join("index.html"))?;
            }
            if source.starts_with("browser-container") {
                // Nginx includes an unrelated error page beside the copied web root.
                fs::write(Path::new(directory).join("50x.html"), b"nginx error page")?;
            }
            fs::create_dir_all(Path::new(directory).join("pkg"))?;
            for (name, shader) in ["ordinary", "mixing"]
                .into_iter()
                .zip(aoe_rendering::shader_data::browser_shaders())
            {
                if name == "mixing"
                    && self
                        .missing_shader_in
                        .borrow()
                        .as_ref()
                        .is_some_and(|container| source.starts_with(container))
                {
                    continue;
                }
                fs::write(
                    Path::new(directory).join(format!("pkg/{name}.wgsl")),
                    shader,
                )?;
            }
        }
        Ok(())
    }
}

fn release_evidence(root: &Path) {
    release_evidence_for(root, &"a".repeat(40));
}

fn release_evidence_for(root: &Path, revision: &str) {
    fs::create_dir_all(root.join("reports/e2e")).expect("E2E directory");
    fs::create_dir_all(root.join("reports/perf")).expect("performance directory");
    fs::write(
        root.join("reports/e2e/pass.json"),
        serde_json::to_vec(&serde_json::json!({"revision":revision,"result":"PASS"}))
            .expect("JSON"),
    )
    .expect("E2E report");
    fs::write(
        root.join("reports/perf/ci.json"),
        serde_json::to_vec(
            &serde_json::json!({"revision":revision,"verdict":"PASS","dirty":false}),
        )
        .expect("JSON"),
    )
    .expect("performance report");
}

#[test]
fn release_build_publishes_one_immutable_bundle_and_verifies_all_evidence() {
    let temp = tempfile::tempdir().expect("checkout");
    release_evidence(temp.path());
    let runtime = FakeRuntime::default();
    build_with(&runtime, temp.path()).expect("release build");
    let manifest_path = temp
        .path()
        .join("reports/release")
        .join("a".repeat(40))
        .join("manifest.json");
    let manifest = load_and_verify_with(&runtime, &manifest_path).expect("verified release");
    assert_eq!(manifest.server_image_id, "sha256:server");
    assert_eq!(manifest.browser_image_id, "sha256:browser");
    assert_eq!(
        runtime
            .calls
            .borrow()
            .iter()
            .filter(|args| args.get(1).is_some_and(|arg| arg == "build"))
            .count(),
        3
    );
    assert!(build_with(&runtime, temp.path()).is_err());
    fs::write(
        manifest_path.parent().expect("parent").join("perf.json"),
        b"tamper",
    )
    .expect("tamper");
    assert!(load_and_verify_with(&runtime, &manifest_path).is_err());
}

#[test]
fn release_build_rejects_dirty_or_wrong_branch_and_cleans_failed_staging() {
    let temp = tempfile::tempdir().expect("checkout");
    release_evidence(temp.path());
    let runtime = FakeRuntime::default();
    runtime.dirty.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    runtime.dirty.set(false);
    runtime.wrong_branch.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    runtime.wrong_branch.set(false);
    runtime.fail_copy.set(true);
    assert!(build_with(&runtime, temp.path()).is_err());
    let release_dir = temp.path().join("reports/release");
    assert_eq!(
        fs::read_dir(&release_dir)
            .expect("release directory")
            .count(),
        0
    );
    assert!(runtime.calls.borrow().iter().any(|args| {
        args.get(1).is_some_and(|arg| arg == "rm")
            && args.iter().any(|arg| arg == "temporary-container")
    }));
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    Tracked,
    Untracked,
    Index,
    Revision,
    SameTreeRevision,
    Branch,
    E2e,
    Perf,
    Bundle,
    ProbeBundle,
    ProbeReport,
}

struct GitRuntime {
    root: PathBuf,
    docker: FakeRuntime,
    mutation: Mutation,
    changed: Cell<bool>,
}

impl GitRuntime {
    fn git(&self, args: &[&str]) -> Result<String> {
        let result = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()?;
        if !result.status.success() {
            return Err(
                format!("git {args:?}: {}", String::from_utf8_lossy(&result.stderr)).into(),
            );
        }
        Ok(String::from_utf8(result.stdout)?.trim().to_owned())
    }

    fn mutate(&self) -> Result<()> {
        match self.mutation {
            Mutation::Tracked | Mutation::Index | Mutation::Revision => {
                fs::write(self.root.join("source.txt"), b"changed")?;
                if matches!(self.mutation, Mutation::Index | Mutation::Revision) {
                    self.git(&["add", "source.txt"])?;
                }
                if matches!(self.mutation, Mutation::Revision) {
                    self.git(&["commit", "-m", "concurrent source change"])?;
                }
            }
            Mutation::Untracked => fs::write(self.root.join("new-source.txt"), b"new")?,
            Mutation::SameTreeRevision => {
                self.git(&["commit", "--allow-empty", "-m", "same tree new revision"])?;
            }
            Mutation::Branch => {
                self.git(&["checkout", "-b", "other"])?;
            }
            Mutation::E2e | Mutation::Perf => {
                let path = self.root.join(if matches!(self.mutation, Mutation::E2e) {
                    "reports/e2e/pass.json"
                } else {
                    "reports/perf/ci.json"
                });
                let mut report: Value = serde_json::from_slice(&fs::read(&path)?)?;
                report["concurrent"] = Value::Bool(true);
                fs::write(path, serde_json::to_vec(&report)?)?;
            }
            Mutation::None | Mutation::Bundle | Mutation::ProbeBundle | Mutation::ProbeReport => {}
        }
        Ok(())
    }
}

impl Runtime for GitRuntime {
    fn output(&self, program: &str, args: &[&str]) -> Result<String> {
        if program == "git" {
            let result = self.git(args)?;
            if matches!(args, ["rev-parse", "HEAD^{tree}"])
                && matches!(self.mutation, Mutation::ProbeBundle | Mutation::ProbeReport)
            {
                let calls = self.docker.calls.borrow();
                if let Some(copy) = calls.iter().find(|call| call[1] == "cp") {
                    let bundle = Path::new(&copy[3]);
                    let report = bundle.parent().ok_or("bundle parent")?.join("e2e.json");
                    if report.exists() {
                        if matches!(self.mutation, Mutation::ProbeBundle) {
                            fs::write(bundle.join("index.html"), b"final probe tamper")?;
                        } else {
                            let mut bytes = fs::read(&report)?;
                            bytes.push(b'\n');
                            fs::write(report, bytes)?;
                        }
                    }
                }
            }
            return Ok(result);
        }
        let result = self.docker.output(program, args)?;
        if program == "rustc" && matches!(self.mutation, Mutation::Bundle) {
            let calls = self.docker.calls.borrow();
            let copy = calls
                .iter()
                .find(|call| call.get(1).is_some_and(|arg| arg == "cp"))
                .ok_or("missing bundle copy")?;
            fs::write(Path::new(&copy[3]).join("index.html"), b"concurrent tamper")?;
        }
        Ok(result)
    }

    fn checked(&self, program: &str, args: &[&str]) -> Result<()> {
        self.docker.checked(program, args)?;
        if matches!(args, ["build", ..]) && !self.changed.replace(true) {
            self.mutate()?;
        }
        Ok(())
    }
}

fn git_fixture(mutation: Mutation) -> (tempfile::TempDir, GitRuntime, String) {
    let temp = tempfile::tempdir().expect("checkout");
    let runtime = GitRuntime {
        root: temp.path().to_owned(),
        docker: FakeRuntime::default(),
        mutation,
        changed: Cell::new(false),
    };
    runtime.git(&["init", "-b", "dev"]).expect("init");
    runtime
        .git(&["config", "user.name", "Release test"])
        .expect("name");
    runtime
        .git(&["config", "user.email", "release@example.invalid"])
        .expect("email");
    runtime
        .git(&["config", "commit.gpgsign", "false"])
        .expect("unsigned fixture");
    fs::write(temp.path().join(".gitignore"), "reports/\n").expect("ignore reports");
    fs::write(temp.path().join("source.txt"), b"source").expect("source");
    runtime.git(&["add", "."]).expect("add");
    runtime.git(&["commit", "-m", "fixture"]).expect("commit");
    let revision = runtime.git(&["rev-parse", "HEAD"]).expect("revision");
    release_evidence_for(temp.path(), &revision);
    (temp, runtime, revision)
}

fn assert_no_release(root: &Path) {
    assert_eq!(
        fs::read_dir(root.join("reports/release"))
            .expect("release directory")
            .count(),
        0
    );
}

#[test]
fn release_rejects_concurrent_git_changes_without_publishing() {
    for mutation in [
        Mutation::Tracked,
        Mutation::Untracked,
        Mutation::Index,
        Mutation::Revision,
        Mutation::SameTreeRevision,
        Mutation::Branch,
    ] {
        let (temp, runtime, _) = git_fixture(mutation);
        let error = build_with(&runtime, temp.path()).expect_err("source instability rejected");
        assert!(
            error.to_string().contains("release"),
            "{mutation:?}: {error}"
        );
        assert_no_release(temp.path());
    }
}

#[test]
fn release_rejects_changed_report_copies_and_staged_artifacts() {
    for mutation in [
        Mutation::E2e,
        Mutation::Perf,
        Mutation::Bundle,
        Mutation::ProbeBundle,
        Mutation::ProbeReport,
    ] {
        let (temp, runtime, _) = git_fixture(mutation);
        let error = build_with(&runtime, temp.path()).expect_err("changed bytes rejected");
        let expected = if matches!(mutation, Mutation::Bundle | Mutation::ProbeBundle) {
            "bundle changed"
        } else {
            "report changed"
        };
        assert!(
            error.to_string().contains(expected),
            "{mutation:?}: {error}"
        );
        assert_no_release(temp.path());
    }
}

#[test]
fn release_checks_the_building_git_worktree() {
    let (repository, mut runtime, revision) = git_fixture(Mutation::Tracked);
    let linked = tempfile::tempdir().expect("linked worktree");
    runtime
        .git(&["checkout", "-b", "fixture-main"])
        .expect("main branch");
    runtime
        .git(&[
            "worktree",
            "add",
            linked.path().to_str().expect("worktree path"),
            "dev",
        ])
        .expect("linked checkout");
    runtime.root = linked.path().to_owned();
    release_evidence_for(linked.path(), &revision);
    assert!(build_with(&runtime, linked.path()).is_err());
    assert_no_release(linked.path());
    assert_eq!(
        fs::read(repository.path().join("source.txt")).expect("main source"),
        b"source"
    );
}

#[test]
fn release_accepts_stable_git_fixture_and_verifies_copied_bytes() {
    let (temp, runtime, revision) = git_fixture(Mutation::None);
    build_with(&runtime, temp.path()).expect("stable release");
    let path = temp
        .path()
        .join("reports/release")
        .join(&revision)
        .join("manifest.json");
    let manifest = load_and_verify_with(&runtime, &path).expect("copied evidence verifies");
    assert_eq!(manifest.source_commit, revision);
    assert_eq!(
        manifest.source_tree,
        runtime.git(&["rev-parse", "HEAD^{tree}"]).expect("tree")
    );
    assert_eq!(runtime.git(&["status", "--porcelain"]).expect("status"), "");
}

#[path = "tests/content.rs"]
mod content_tests;
