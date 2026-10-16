//! Fresh local intent records. Reopened files never mint a lease or cleanup authority.
use super::{Operation, transport::Cid};
use crate::gates::runner::evidence::PrivateOutput;
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum Event {
    Intent,
    Created,
    Started,
    WorkloadObserved,
    CleanupAttempt,
    VerifiedAbsent,
    Incomplete,
}
#[derive(Serialize)]
struct Record {
    schema: u32,
    authoritative: bool,
    domain: &'static str,
    nonce: String,
    source_seal: String,
    template_blake3: String,
    operation: &'static str,
    cid: Option<String>,
    events: Vec<Event>,
}
pub(super) struct Journal<'a> {
    output: &'a PrivateOutput,
    name: String,
    record: Record,
    endpoint: Option<(File, Vec<u8>)>,
}
pub(super) fn nonce() -> Result<String, std::io::Error> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
impl<'a> Journal<'a> {
    pub(super) fn new(
        output: &'a PrivateOutput,
        nonce: String,
        seal: &str,
        template: &str,
        op: &Operation,
    ) -> Result<Self, &'static str> {
        if [seal, template].iter().any(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err("input seal invalid");
        }
        if nonce.len() != 32
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("nonce invalid");
        }
        let name = format!("worker-{nonce}.json");
        if fs::symlink_metadata(output.directory().join(&name)).is_ok() {
            return Err("journal already exists; quarantine");
        }
        Ok(Self {
            output,
            name,
            record: Record {
                schema: 1,
                authoritative: false,
                domain: "LOCAL_WORKER_LIFECYCLE_V1",
                nonce,
                source_seal: seal.into(),
                template_blake3: template.into(),
                operation: op.argument(),
                cid: None,
                events: vec![],
            },
            endpoint: None,
        })
    }
    pub(super) fn append(&mut self, event: Event, cid: Option<&Cid>) -> Result<(), &'static str> {
        self.verify()?;
        if self.record.events.len() >= 16 {
            return Err("journal budget");
        }
        if let Some(cid) = cid {
            if self
                .record
                .cid
                .as_ref()
                .is_some_and(|old| old != cid.value())
            {
                return Err("journal CID changed");
            }
            self.record.cid = Some(cid.value().into());
        }
        self.record.events.push(event);
        let bytes = serde_json::to_vec(&self.record).map_err(|_| "journal serialization")?;
        let path = self
            .output
            .atomic(&self.name, &bytes)
            .map_err(|_| "journal persistence")?;
        let mut file = Self::open(&path)?;
        let mut reopened = Vec::new();
        file.by_ref()
            .take(8193)
            .read_to_end(&mut reopened)
            .map_err(|_| "journal reread")?;
        if reopened != bytes {
            return Err("journal changed after publish");
        }
        self.endpoint = Some((file, bytes));
        self.verify()
    }
    #[cfg(unix)]
    fn open(path: &std::path::Path) -> Result<File, &'static str> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| "journal open")?;
        let m = file.metadata().map_err(|_| "journal metadata")?;
        if !m.is_file() || m.nlink() != 1 || m.mode() & 0o7777 != 0o600 {
            return Err("journal hygiene");
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    fn open(_: &std::path::Path) -> Result<File, &'static str> {
        Err("Unix journal unavailable")
    }
    #[cfg(not(unix))]
    pub(super) fn verify(&mut self) -> Result<(), &'static str> {
        Err("Unix journal unavailable")
    }
    #[cfg(unix)]
    pub(super) fn verify(&mut self) -> Result<(), &'static str> {
        use std::os::unix::fs::MetadataExt;
        let path: PathBuf = self.output.directory().join(&self.name);
        if let Some((file, bytes)) = &mut self.endpoint {
            let endpoint = fs::symlink_metadata(&path).map_err(|_| "journal endpoint")?;
            let held = file.metadata().map_err(|_| "journal handle")?;
            if !endpoint.is_file()
                || endpoint.nlink() != 1
                || endpoint.mode() & 0o7777 != 0o600
                || endpoint.dev() != held.dev()
                || endpoint.ino() != held.ino()
                || endpoint.len() != held.len()
            {
                return Err("journal endpoint changed");
            }
            file.seek(SeekFrom::Start(0)).map_err(|_| "journal seek")?;
            let mut current = Vec::new();
            file.by_ref()
                .take(8193)
                .read_to_end(&mut current)
                .map_err(|_| "journal verify")?;
            if &current != bytes {
                return Err("journal bytes changed");
            }
        } else if fs::symlink_metadata(path).is_ok() {
            return Err("journal substituted before intent");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
