//! What compare and showcase ask the dev server: its URL when it is current,
//! and the reason when it is not.
use super::{Health, commit, git, repository};
use crate::devserver::{
    layout::Layout,
    play_url, process, serves_base,
    state::{Instance, State},
};
use std::path::Path;

/// A recorded play server whose process is `sleep` started as its binary and
/// whose port is a stand-in answering `/health` with `build`.
struct Recorded {
    pid: u32,
    binary: std::path::PathBuf,
    _health: Health,
}

impl Drop for Recorded {
    fn drop(&mut self) {
        process::stop(self.pid, &self.binary);
    }
}

fn record(root: &Path, revision: &str, served: &str, alive: bool) -> Recorded {
    let layout = Layout::resolve(root).expect("layout");
    let built = crate::devserver::build::Built::of(&layout, revision);
    std::fs::create_dir_all(&built.tree).expect("tree");
    // Keep argv unique without opening an executable for writing during parallel forks.
    if built.server.exists() {
        std::fs::remove_file(&built.server).expect("previous sleep fixture");
    }
    std::os::unix::fs::symlink("/bin/sleep", &built.server).expect("sleep fixture");
    let health = Health::serving(served, "[]");
    let pid = if alive {
        process::spawn(
            &built.server,
            &["120"],
            &built.tree,
            &[],
            &built.tree.join("log"),
        )
        .expect("spawn")
    } else {
        4_000_000
    };
    let mut state = State::default();
    state.replace(Instance {
        name: "play".into(),
        port: health.port,
        pid,
        started: 0,
        revision: revision.into(),
        identity: "x".into(),
        container: None,
    });
    state.write(&layout.state()).expect("state");
    Recorded {
        pid,
        binary: built.server,
        _health: health,
    }
}

#[test]
fn showcase_uses_the_server_when_it_is_healthy_at_the_revision_dev_is_at() {
    let (_directory, root) = repository();
    let head = git(&root, &["rev-parse", "HEAD"]);
    let recorded = record(&root, &head, &head, true);
    let url = play_url(&root).expect("a current server is used");
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    drop(recorded);
}

#[test]
fn showcase_falls_back_and_says_why_when_nothing_runs() {
    let (_directory, root) = repository();
    let why = play_url(&root).expect_err("no server");
    assert!(
        why.contains("not running") && why.contains("make dev-server"),
        "{why}"
    );
}

#[test]
fn showcase_falls_back_when_the_process_is_gone_or_stale_or_silent() {
    let (_directory, root) = repository();
    let old = git(&root, &["rev-parse", "HEAD"]);
    let gone = record(&root, &old, &old, false);
    assert!(
        play_url(&root)
            .expect_err("gone")
            .contains("process is gone")
    );
    drop(gone);
    let alive = record(&root, &old, &old, true);
    let new = commit(&root, "file.txt", "dev moved");
    let why = play_url(&root).expect_err("stale");
    assert!(
        why.contains(&old[..12]) && why.contains(&new[..12]),
        "{why}"
    );
    drop(alive);
    let silent = record(&root, &new, "somebody-else", true);
    let why = play_url(&root).expect_err("serves another build");
    assert!(
        why.contains("somebody-els") || why.contains("serves"),
        "{why}"
    );
    drop(silent);
}

#[test]
fn compare_uses_the_shared_build_only_for_the_revision_dev_is_at() {
    let (_directory, root) = repository();
    let base = git(&root, &["rev-parse", "HEAD"]);
    assert!(serves_base(&root, &base).is_ok());
    commit(&root, "file.txt", "dev moved");
    let why = serves_base(&root, &base).expect_err("dev moved past the merge base");
    assert!(why.contains("merge base"), "{why}");
}
