use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) type State = Arc<Mutex<Option<Progress>>>;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    PreparingSources,
    DownloadingSource,
    SamplingOverview,
    SamplingWater,
    BuildingPyramids,
    PublishingPackage,
    VerifyingPackage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Unit {
    Bytes,
    Pages,
}
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Progress {
    version: u8,
    pub phase: Phase,
    detail: Option<String>,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    unit: Option<Unit>,
}
impl Progress {
    fn valid(&self) -> bool {
        self.version == 1
            && self.detail.as_ref().is_none_or(|value| value.len() <= 64)
            && match (self.completed, self.total, self.unit) {
                (None, None, None) => true,
                (Some(done), Some(total), Some(_)) => {
                    total > 0 && total <= 1_000_000_000_000 && done <= total
                }
                _ => false,
            }
    }
}
pub(crate) fn stage(state: &State, phase: Phase) {
    if let Ok(mut value) = state.lock() {
        *value = Some(Progress {
            version: 1,
            phase,
            detail: None,
            completed: None,
            total: None,
            unit: None,
        });
    }
}
pub(crate) fn snapshot(state: &State) -> Option<Progress> {
    state.lock().ok()?.clone()
}

pub(super) struct Monitor {
    path: PathBuf,
    state: State,
    last_read: Option<Instant>,
}
impl Monitor {
    pub(super) fn new(path: PathBuf, state: State) -> Self {
        Self {
            path,
            state,
            last_read: None,
        }
    }
    pub(super) fn poll(&mut self) {
        if self
            .last_read
            .is_some_and(|last| last.elapsed() < Duration::from_millis(200))
        {
            return;
        }
        self.last_read = Some(Instant::now());
        let read = || -> Option<Progress> {
            if !std::fs::symlink_metadata(&self.path)
                .ok()?
                .file_type()
                .is_file()
            {
                return None;
            }
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW);
            }
            let file = options.open(&self.path).ok()?;
            if !file.metadata().ok()?.is_file() {
                return None;
            }
            let mut bytes = Vec::new();
            file.take(4097).read_to_end(&mut bytes).ok()?;
            if bytes.len() > 4096 {
                return None;
            }
            let progress: Progress = serde_json::from_slice(&bytes).ok()?;
            progress.valid().then_some(progress)
        };
        if let Some(progress) = read()
            && let Ok(mut state) = self.state.lock()
        {
            *state = Some(progress);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_oversized_progress_cannot_replace_last_valid_counter() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("progress.json");
        let state = State::default();
        let mut monitor = Monitor::new(path.clone(), state.clone());
        let valid = br#"{"version":1,"phase":"building_pyramids","detail":null,"completed":4,"total":8,"unit":"pages"}"#;
        std::fs::write(&path, valid).unwrap();
        monitor.poll();
        let before = snapshot(&state).unwrap();
        assert_eq!(before.completed, Some(4));
        for invalid in [
            vec![b' '; 4097],
            b"{".to_vec(),
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("\"total\":8", "\"total\":2")
                .into_bytes(),
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("building_pyramids", "unknown_phase")
                .into_bytes(),
        ] {
            std::fs::write(&path, invalid).unwrap();
            monitor.last_read = None;
            monitor.poll();
            assert_eq!(snapshot(&state), Some(before.clone()));
        }
        stage(&state, Phase::VerifyingPackage);
        assert_eq!(snapshot(&state).unwrap().phase, Phase::VerifyingPackage);
        assert_eq!(snapshot(&state).unwrap().completed, None);
    }
}
