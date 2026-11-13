//! Owned compilation competing with the measured browser; no synthetic score.
use super::*;
use std::os::unix::process::CommandExt;
#[cfg(test)]
mod tests;

pub(super) struct CompileLoad {
    child: std::process::Child,
    _target: tempfile::TempDir,
}

fn fresh_target(root: &std::path::Path) -> Result<tempfile::TempDir> {
    fs::create_dir_all(root)?;
    Ok(tempfile::Builder::new()
        .prefix("load-target-")
        .tempdir_in(root)?)
}
impl CompileLoad {
    pub(super) fn start(log: &std::path::Path) -> Result<Self> {
        let output = fs::File::create(log)?;
        let target = fresh_target(std::path::Path::new(".cache/client-frame-bench"))?;
        let directory = target.path().canonicalize()?;
        let seed = directory
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid load target name")?;
        let child = Command::new("make")
            .arg("--no-print-directory")
            .arg("client-frame-bench-compile-load")
            .arg(format!("CLIENT_FRAME_LOAD_TARGET={}", directory.display()))
            // An unused unique cfg forces real compilation rather than an
            // object-cache hit, without changing any source or production gate.
            .arg(format!(
                "CLIENT_FRAME_LOAD_RUSTFLAGS=--cfg=aoe_client_frame_load=\"{seed}\""
            ))
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(output.try_clone()?)
            .stderr(output)
            .spawn()?;
        let mut load = Self {
            child,
            _target: target,
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            load.live()?;
            if fs::read_to_string(log)?.contains("Compiling ") {
                return Ok(load);
            }
            if Instant::now() >= deadline {
                return Err("compile load did not start compiling within 60s".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub(super) fn live(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            return Err(format!(
                "compile load ended before the loaded campaign completed: {status}"
            )
            .into());
        }
        Ok(())
    }
}
impl Drop for CompileLoad {
    fn drop(&mut self) {
        // SIGTERM reaches Make's process group, including Docker's owned runner.
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.child.id() as i32),
            nix::sys::signal::SIGTERM,
        );
        let _ = self.child.wait();
    }
}
