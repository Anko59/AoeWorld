//! The dev server's record of what it runs: written under the lock, read by
//! anyone. Health on the port, not this file, decides whether a server is up.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// One server process of the dev server.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Instance {
    /// `play` (no map open, presets prepared) or `view-<id>` (one map open).
    pub(crate) name: String,
    pub(crate) port: u16,
    pub(crate) pid: u32,
    pub(crate) started: u64,
    pub(crate) revision: String,
    /// What the instance was started with; a change restarts it.
    pub(crate) identity: String,
    /// The container the server runs in; none for a plain process.
    pub(crate) container: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) instances: Vec<Instance>,
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl State {
    pub(crate) fn read(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Whole-file replace, so a reader never sees half of it.
    pub(crate) fn write(&self, path: &Path) -> Result<(), String> {
        let temporary = path.with_extension("tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    pub(crate) fn find(&self, name: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.name == name)
    }

    pub(crate) fn replace(&mut self, instance: Instance) {
        self.instances.retain(|i| i.name != instance.name);
        self.instances.push(instance);
    }
}
