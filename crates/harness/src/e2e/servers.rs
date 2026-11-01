//! One disposable game server per Playwright worker (#388).
//!
//! A server owns shared map and unit state, so
//! workers sharing a server would trample each other. The run starts one server
//! per worker, each on its own free port and with its own copy of the map store,
//! and lists their URLs for the browser container.
use super::{Server, landscape, ready, stale};
use crate::browser_ports;
use std::{
    net::SocketAddr,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

/// Workers (and servers) of a run that does not name a count.
const DEFAULT_WORKERS: u32 = 3;
/// Most workers one run may use; each is a server and a browser on this host.
const MAX_WORKERS: u32 = 8;

type Failure = Box<dyn std::error::Error>;

/// The worker count named by `AOE_E2E_WORKERS`, absent or empty meaning three.
/// The coverage run keeps one instrumented server and one worker, and the
/// duel's focused specs run on one.
pub(super) fn worker_count(coverage: Option<&str>, focused: bool) -> Result<u32, String> {
    select(
        &std::env::var("AOE_E2E_WORKERS").unwrap_or_default(),
        coverage,
        focused,
    )
}

fn select(text: &str, coverage: Option<&str>, focused: bool) -> Result<u32, String> {
    let count = if text.is_empty() {
        DEFAULT_WORKERS
    } else {
        text.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| text.parse::<u32>().ok())
            .flatten()
            .filter(|count| (1..=MAX_WORKERS).contains(count))
            .ok_or_else(|| format!("AOE_E2E_WORKERS must be 1..={MAX_WORKERS}, got {text:?}"))?
    };
    if coverage.is_some() {
        return if text.is_empty() || count == 1 {
            Ok(1)
        } else {
            Err("AOE_E2E_COVERAGE runs one server and one worker; unset AOE_E2E_WORKERS".into())
        };
    }
    Ok(if focused { 1 } else { count })
}

/// The `AOE_BASE_URLS` value: the servers' URLs in worker order.
pub(super) fn base_urls(addresses: &[SocketAddr]) -> String {
    addresses
        .iter()
        .map(|address| format!("http://{address}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Running servers, each with the map store it owns. Dropping kills them.
pub(super) struct Fleet {
    pub(super) addresses: Vec<SocketAddr>,
    pub(super) hash: String,
    servers: Vec<Server>,
    _fixtures: Vec<landscape::Fixture>,
}

impl Fleet {
    pub(super) fn start(
        root: &Path,
        binary: &Path,
        existing_maps: &Path,
        count: u32,
    ) -> Result<Self, Failure> {
        let mut fleet = Self {
            addresses: Vec::new(),
            hash: String::new(),
            servers: Vec::new(),
            _fixtures: Vec::new(),
        };
        // Hold every reservation before starting children: a newly spawned
        // server may not have bound yet when the next worker is allocated.
        for listener in browser_ports::configured(count)? {
            let address = listener.local_addr()?;
            let fixture = landscape::prepare(root, Some(existing_maps))
                .map_err(|error| stale::annotate(error, existing_maps))?;
            drop(listener);
            let child = Command::new(binary)
                .current_dir(root)
                .env("AOE_BIND", address.to_string())
                .env("AOE_SCENARIO", "smoke")
                .env("AOE_MAP_PACKAGE_DIRECTORY", fixture.directory.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?;
            fleet.hash = fixture.hash.clone();
            fleet.servers.push(Server(child));
            fleet.addresses.push(address);
            fleet._fixtures.push(fixture);
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        for (index, address) in fleet.addresses.iter().enumerate() {
            while !ready(*address) {
                if let Some(status) = fleet.servers[index].0.try_wait()? {
                    return Err(format!("test server {index} exited early: {status}").into());
                }
                if Instant::now() >= deadline {
                    return Err(
                        format!("test server {index} did not become healthy within 20s").into(),
                    );
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(fleet)
    }

    /// The first server that has exited, as an error naming its status.
    pub(super) fn check_alive(&mut self) -> Result<(), Failure> {
        for (index, server) in self.servers.iter_mut().enumerate() {
            if let Some(status) = server.0.try_wait()? {
                return Err(
                    format!("test server {index} exited during browser run: {status}").into(),
                );
            }
        }
        Ok(())
    }

    /// Stops every server, reporting the first that did not exit gracefully.
    pub(super) fn shutdown(&mut self) -> Result<(), Failure> {
        let mut first = Ok(());
        for server in &mut self.servers {
            let result = server.shutdown();
            if first.is_ok() {
                first = result;
            }
        }
        first
    }
}
