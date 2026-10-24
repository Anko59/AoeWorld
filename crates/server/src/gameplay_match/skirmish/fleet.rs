//! The AI processes of a skirmish: a few Python host processes, each playing
//! several slots over the ordinary gameplay socket (`python -m aoe_ai host`),
//! started by the server and killed with the match.
use std::{
    io::Write,
    process::{Child, Command, Stdio},
};

/// Slots one host process plays; 99 opponents take 8 processes, the split
/// `make scale-match` measured (docs/scale/match.md).
pub(super) const BOTS_PER_HOST: usize = 13;
/// Upper bound on a host's life, so a lost server never leaves one behind.
const HOST_MAX_SECONDS: u32 = 6 * 60 * 60;

/// How the server starts a host: `AOE_AI_COMMAND` and its own address.
#[derive(Clone, Debug)]
pub(crate) struct Launch {
    pub command: Vec<String>,
    pub url: String,
}

/// The running hosts of one match. Dropping it kills them.
pub(super) struct Fleet {
    pub world_id: u64,
    children: Vec<Child>,
}

impl Launch {
    /// The arguments of one host process (tokens arrive on its stdin).
    pub(super) fn arguments(&self) -> Vec<String> {
        let mut arguments = self.command[1..].to_vec();
        arguments.extend(
            [
                "--url",
                &self.url,
                "--tokens-file",
                "-",
                "--max-seconds",
                &HOST_MAX_SECONDS.to_string(),
            ]
            .map(str::to_owned),
        );
        arguments
    }

    /// Starts `tokens.len().div_ceil(BOTS_PER_HOST)` hosts for `world_id`.
    pub(super) fn spawn(&self, world_id: u64, tokens: &[String]) -> Result<Fleet, String> {
        let mut fleet = Fleet {
            world_id,
            children: Vec::new(),
        };
        for group in tokens.chunks(BOTS_PER_HOST) {
            let mut child = Command::new(&self.command[0])
                .args(self.arguments())
                .stdin(Stdio::piped())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|error| format!("could not start the AI host: {error}"))?;
            let tokens = serde_json::to_vec(group).map_err(|error| error.to_string())?;
            let written = child
                .stdin
                .take()
                .ok_or_else(|| "the AI host has no input".to_owned())
                .and_then(|mut input| input.write_all(&tokens).map_err(|e| e.to_string()));
            fleet.children.push(child);
            written.map_err(|error| format!("could not hand the AI host its slots: {error}"))?;
        }
        tracing::info!(
            hosts = fleet.children.len(),
            bots = tokens.len(),
            "skirmish AI hosts started"
        );
        Ok(fleet)
    }
}

impl Fleet {
    /// Whether every host process has exited.
    pub(super) fn finished(&mut self) -> bool {
        self.children
            .iter_mut()
            .all(|child| child.try_wait().ok().flatten().is_some())
    }

    pub(super) fn hosts(&self) -> usize {
        self.children.len()
    }
}

impl Drop for Fleet {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        if !self.children.is_empty() {
            tracing::info!(hosts = self.children.len(), "skirmish AI hosts stopped");
        }
    }
}
