mod bash;
mod hooks;
mod stop;
mod vcs;

use super::{context::Context, role::Role};
use std::{fs, path::Path, process::Command};

/// A Git checkout with the protected classes and a few role-owned files.
pub(super) struct Fixture {
    pub(super) temp: tempfile::TempDir,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary checkout");
        let root = temp.path();
        let status = Command::new("git")
            .args(["init", "-q", "-b", "feature"])
            .current_dir(root)
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .status()
            .expect("git init");
        assert!(status.success());
        for file in [
            "crates/map/src/lib.rs",
            "crates/map/src/tests.rs",
            "crates/map/tests/terrain.rs",
            "crates/harness/src/main.rs",
            "gates/registry.json",
            "baselines/perf/instructions.json",
            "browser/tests/play.spec.ts",
            "docs/index.md",
            "Makefile",
        ] {
            let path = root.join(file);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, "x\n").expect("file");
        }
        Self { temp }
    }

    pub(super) fn root(&self) -> &Path {
        self.temp.path()
    }

    pub(super) fn context(&self, role: Role) -> Context {
        Context::new(self.root(), role, None).expect("context")
    }
}

pub(super) const ROLES: [Role; 5] = [
    Role::Main,
    Role::Tester,
    Role::Implementer,
    Role::Reviewer,
    Role::Other,
];

pub(super) fn judge(fixture: &Fixture, role: Role, command: &str) -> Result<(), String> {
    let context = fixture.context(role);
    let root = context.root.clone();
    super::bash::judge(&context, command, Some(&root))
}

#[track_caller]
pub(super) fn denied(fixture: &Fixture, role: Role, command: &str) {
    assert!(
        judge(fixture, role, command).is_err(),
        "expected {role:?} to be denied `{command}`"
    );
}

#[track_caller]
pub(super) fn allowed(fixture: &Fixture, role: Role, command: &str) {
    if let Err(reason) = judge(fixture, role, command) {
        panic!("expected {role:?} to be allowed `{command}`: {reason}");
    }
}
