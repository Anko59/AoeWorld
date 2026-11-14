//! Real compilation inside the measurement container and its existing caps.
use super::Result;
use std::{
    fs,
    io::Read,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct CompileLoad {
    child: Child,
    _target: tempfile::TempDir,
    pub log: PathBuf,
}
impl CompileLoad {
    pub fn start() -> Result<Self> {
        let root = std::path::Path::new(".cache/quality-compile-load");
        fs::create_dir_all(root)?;
        let target = tempfile::Builder::new()
            .prefix("target-")
            .tempdir_in(root)?;
        let directory = target.path().canonicalize()?;
        let seed = directory
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid load target")?;
        let log = root.join(format!("{seed}.log"));
        let mut command = Command::new("cargo");
        command
            .args([
                "build",
                "--locked",
                "--release",
                "-p",
                "aoe-harness",
                "--target-dir",
            ])
            .arg(&directory)
            // An unused unique cfg prevents object-cache hits. It is never
            // read by gameplay, whose already-built binary is being measured.
            .env(
                "RUSTFLAGS",
                format!("--cfg=aoe_quality_compile_load=\"{seed}\""),
            );
        Self::spawn(&mut command, target, log)
    }
    pub fn spawn(command: &mut Command, target: tempfile::TempDir, log: PathBuf) -> Result<Self> {
        let output = fs::File::create(&log)?;
        let child = command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .spawn()?;
        let mut load = Self {
            child,
            _target: target,
            log,
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            load.live()?;
            let mut text = String::new();
            fs::File::open(&load.log)?
                .take(65_536)
                .read_to_string(&mut text)?;
            if text.contains("Compiling ") {
                return Ok(load);
            }
            if Instant::now() >= deadline {
                return Err("compile load did not begin compiling within 60s".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn live(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            return Err(
                format!("compile load ended before measurement completed: {status}").into(),
            );
        }
        Ok(())
    }
}
impl Drop for CompileLoad {
    fn drop(&mut self) {
        // All compiler descendants inherit this owned group. Artifacts are
        // disposable; stop the load before removing its temporary target.
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.child.id() as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
        let _ = self.child.wait();
    }
}
