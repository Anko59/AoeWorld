//! Offline CLI fixture. Copy beneath crates/harness/tests/task_io_cli/fixture.rs.
//! Fake provider/transport observations are never SDK or authenticated role proof.
use serde_json::{Value, json};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};
pub const GUIDES: &[&str] = &[
    "AGENTS.md",
    "docs/agent-engineering.md",
    "docs/testing.md",
    "docs/qa.md",
    "docs/adr/0006-provider-neutral-harness.md",
    "skills/harness-ci/SKILL.md",
    "skills/performance/SKILL.md",
    "skills/protocol/SKILL.md",
    "skills/release/SKILL.md",
    "skills/simulation-server/SKILL.md",
    "skills/wasm-rendering/SKILL.md",
    "skills/game-assets/SKILL.md",
    "skills/asset-import/SKILL.md",
];
pub fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .current_dir(root)
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
pub struct Fixture {
    pub owner: tempfile::TempDir,
    pub repo: PathBuf,
    pub output: PathBuf,
    pub input: PathBuf,
    pub tools: PathBuf,
    pub task: Value,
    pub base: String,
    pub head: String,
    pub index: Vec<u8>,
    pub config: Vec<u8>,
}
impl Fixture {
    pub fn new() -> Self {
        let owner = tempfile::tempdir().unwrap();
        let repo = owner.path().join("repo");
        let output = owner.path().join("evidence");
        let tools = owner.path().join("tools");
        let input = owner.path().join("task.json");
        for path in [&repo, &output, &tools] {
            fs::create_dir(path).unwrap();
        }
        git(&repo, &["init", "--quiet", "--template="]);
        fs::create_dir(repo.join("gates")).unwrap();
        // Paths assume helper is copied to crates/harness/tests/task_io_cli/.
        fs::write(
            repo.join("gates/registry.json"),
            include_str!("../../../../gates/registry.json"),
        )
        .unwrap();
        fs::write(
            repo.join("gates/roles.json"),
            include_str!("../../../../gates/roles.json"),
        )
        .unwrap();
        for guide in GUIDES {
            let path = repo.join(guide);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("Immutable canonical guide {guide}\n")).unwrap();
        }
        fs::write(repo.join("Makefile"), "all:\n\t@touch SHOULD_NOT_EXIST\n").unwrap();
        fs::write(repo.join("case.rs"), "fn case() { assert_eq!(1, 1); }\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "base"]);
        let base = git(&repo, &["rev-parse", "HEAD"]);
        fs::write(repo.join("case.rs"), "fn case() { assert_eq!(1, 2); }\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "candidate"]);
        let head = git(&repo, &["rev-parse", "HEAD"]);
        let selected = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(&repo)
            .arg("ci-select")
            .env_remove("GITHUB_OUTPUT")
            .env_remove("AOE_BASE_SHA")
            .output()
            .unwrap();
        assert!(
            selected.status.success(),
            "{}",
            String::from_utf8_lossy(&selected.stderr)
        );
        let selection: Value = serde_json::from_slice(&selected.stdout).unwrap();
        let task = json!({"version":1,"id":"io-fixture","kind":"bug","candidate":head,"base":base,
            "registry_hash":selection["registry_hash"],"role":"coordinator","provider":"codex","status":"planned",
            "objective":"Observe immutable task inputs","acceptance":["Full preflight retained"],
            "todo":["Independent review"],"artifacts":[],"rounds_remaining":4});
        let index = fs::read(repo.join(".git/index")).unwrap();
        let config = fs::read(repo.join(".git/config")).unwrap();
        let fixture = Self {
            owner,
            repo,
            output,
            input,
            tools,
            task,
            base,
            head,
            index,
            config,
        };
        fixture.provider("#!/bin/sh\nprintf 'fixture-codex 1.0\\n'\n");
        fixture
    }
    pub fn provider(&self, body: &str) {
        let body = if body.starts_with("#!/bin/sh\n") {
            body.replacen(
                "#!/bin/sh\n",
                "#!/bin/sh\n[ \"$#\" -eq 1 ] && [ \"$1\" = --version ] || exit 19\n",
                1,
            )
        } else {
            body.replacen(
                "#!/usr/bin/python3\n",
                "#!/usr/bin/python3\nimport sys\nassert sys.argv[1:] == ['--version']\n",
                1,
            )
        };
        let path = self.tools.join("codex");
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    pub fn save(&self) {
        fs::write(&self.input, serde_json::to_vec(&self.task).unwrap()).unwrap();
    }
    pub fn stale(&self) {
        fs::write(
            self.output.join("task-plan.json"),
            br#"{"authoritative":true,"status":"READY"}"#,
        )
        .unwrap();
    }
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
        cmd.current_dir(&self.repo)
            .args([
                "task-plan",
                "--task",
                self.input.to_str().unwrap(),
                "--output",
                self.output.to_str().unwrap(),
            ])
            .env("PATH", format!("{}:/usr/bin:/bin", self.tools.display()))
            .env_remove("GITHUB_OUTPUT")
            .env_remove("AOE_BASE_SHA")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN");
        cmd
    }
    pub fn run(&self) -> Output {
        self.save();
        self.stale();
        self.command().output().unwrap()
    }
    pub fn descriptor(&self) -> Value {
        serde_json::from_slice(&fs::read(self.output.join("task-plan.json")).unwrap()).unwrap()
    }
    pub fn preserve(&self) {
        assert_eq!(git(&self.repo, &["rev-parse", "HEAD"]), self.head);
        assert_eq!(fs::read(self.repo.join(".git/index")).unwrap(), self.index);
        assert_eq!(
            fs::read(self.repo.join(".git/config")).unwrap(),
            self.config
        );
        assert!(!self.repo.join("SHOULD_NOT_EXIST").exists());
    }
    pub fn success(&self, out: &Output) -> Value {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let result = self.descriptor();
        assert_eq!(result["status"], "PLANNED_NON_AUTHORITATIVE");
        assert_eq!(result["authoritative"], false);
        assert_eq!(result["plan"]["authoritative"], false);
        assert_eq!(
            result["plan"]["adapter"]["authoritative_role_identity"],
            false
        );
        for field in ["pre_tool_interception", "filesystem_isolation"] {
            assert!(
                result["plan"]["adapter"][field]
                    .as_str()
                    .unwrap()
                    .starts_with("UNAVAILABLE")
            );
        }
        assert!(
            result["plan"]["handoff"]["gates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|gate| gate == "test-unit")
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(self.output.join("task-plan.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        self.preserve();
        result
    }
    pub fn unavailable(&self, out: &Output, reason: &str) -> Value {
        assert!(!out.status.success());
        let result = self.descriptor();
        assert_eq!(result["status"], "UNAVAILABLE");
        assert_eq!(result["authoritative"], false);
        assert!(
            result["reason"].as_str().unwrap().contains(reason),
            "{result}"
        );
        self.preserve();
        result
    }
    pub fn commit_candidate(&mut self) {
        git(&self.repo, &["add", "."]);
        git(
            &self.repo,
            &["commit", "--quiet", "-m", "candidate variant"],
        );
        self.head = git(&self.repo, &["rev-parse", "HEAD"]);
        self.task["candidate"] = json!(self.head);
        self.index = fs::read(self.repo.join(".git/index")).unwrap();
    }
    pub fn freeze_as_base(&mut self) {
        self.base = self.head.clone();
        self.task["base"] = json!(self.base);
        let path = self.repo.join("case.rs");
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(b"// next immutable candidate\n");
        fs::write(path, bytes).unwrap();
        self.commit_candidate();
    }
    pub fn artifact(&mut self, bytes: &[u8]) -> PathBuf {
        let relative = "task-artifacts/io-fixture/result.json";
        let path = self.output.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        self.task["artifacts"] = json!([{"kind":"review","path":relative,"blake3":blake3::hash(bytes).to_hex().to_string()}]);
        path
    }
    pub fn transport(&self, mode: &str) {
        let config = self.owner.path().join("transport.json");
        fs::write(
            &config,
            serde_json::to_vec(&json!({"mode":mode,"marker":self.owner.path().join("started")}))
                .unwrap(),
        )
        .unwrap();
        let template = include_str!("transport.py");
        let body = template.replace(
            "CONFIG_PATH_LITERAL",
            &serde_json::to_string(config.to_str().unwrap()).unwrap(),
        );
        let path = self.tools.join("git");
        fs::write(&path, body).unwrap();
        #[cfg(unix)]
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}
pub fn wait_marker(marker: &Path, child: &mut std::process::Child) {
    // Snapshot setup precedes supervised diff execution; tolerate loaded CI here.
    // The test separately enforces the unchanged five-second diff capture bound.
    let deadline = Instant::now() + Duration::from_secs(60);
    while !marker.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "process exited before marker"
        );
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("marker deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
