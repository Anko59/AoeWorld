//! Fixtures shared by the dev-server tests: a throwaway Git repository whose
//! `origin/dev` can move, and a stand-in server that answers `/health`.
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

mod decisions;
mod fingerprint;
mod lock;
mod presets;
mod processes;
mod usage;
mod warm;

pub(super) fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_owned()
}

/// A repository with one commit as `origin/dev`.
pub(super) fn repository() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("directory");
    let root = directory.path().to_path_buf();
    git(&root, &["init", "--quiet"]);
    commit(&root, "first", "one");
    (directory, root)
}

/// Commit `content` to `file` and move `origin/dev` to it; returns the commit.
pub(super) fn commit(root: &Path, file: &str, content: &str) -> String {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
    std::fs::write(&path, content).expect("write");
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "change"]);
    let head = git(root, &["rev-parse", "HEAD"]);
    git(root, &["update-ref", "refs/remotes/origin/dev", &head]);
    head
}

/// Answers `GET /health` with the given build until dropped.
pub(super) struct Health {
    pub(super) port: u16,
    stop: Arc<AtomicBool>,
}

impl Health {
    pub(super) fn serving(build: &str, jobs: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (build, jobs) = (build.to_owned(), jobs.to_owned());
        thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                };
                stream.set_nonblocking(false).expect("blocking");
                let mut buffer = [0u8; 512];
                let count = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..count]);
                let body = if request.starts_with("GET /health") {
                    format!(r#"{{"status":"ok","build":"{build}"}}"#)
                } else {
                    jobs.clone()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { port, stop }
    }
}

impl Drop for Health {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
