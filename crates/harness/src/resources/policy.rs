//! Shared container caps and immediate memory admission; no slots or load wait.
use std::{fs, io, path::Path};

pub(crate) const FLOOR_KIB: u64 = 3 * 1024 * 1024;
const MEMINFO: &str = "/proc/meminfo";
const LINUX: bool = cfg!(target_os = "linux");
const CAPS: &str = include_str!("../../../../gates/container.args");

pub(crate) fn cap_args() -> Vec<String> {
    CAPS.split_whitespace().map(str::to_owned).collect()
}

pub(crate) fn available_kib(text: &str) -> io::Result<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))
        .and_then(|value| value.split_whitespace().next())
        .ok_or_else(|| io::Error::other("MemAvailable missing"))?
        .parse()
        .map_err(io::Error::other)
}

pub(crate) fn admit(text: &str) -> io::Result<()> {
    let available = available_kib(text)?;
    if available < FLOOR_KIB {
        return Err(io::Error::other(format!(
            "MemAvailable {available} KiB is below the {FLOOR_KIB} KiB floor; refusing immediately, retry later"
        )));
    }
    Ok(())
}

pub(crate) fn check() -> io::Result<()> {
    admit_measured(meminfo()?)
}

/// Work is admitted only where its memory is measured.
pub(crate) fn admit_measured(meminfo: Option<String>) -> io::Result<()> {
    match meminfo {
        Some(text) => admit(&text),
        None => Err(io::Error::other(format!(
            "no {MEMINFO} on this system: containers are admitted inside the Linux VM"
        ))),
    }
}

/// This host's memory; `None` only on a system that is not Linux.
pub(crate) fn meminfo() -> io::Result<Option<String>> {
    meminfo_at(Path::new(MEMINFO), LINUX)
}

pub(crate) fn meminfo_at(path: &Path, linux: bool) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Err(error) if !linux && error.kind() == io::ErrorKind::NotFound => Ok(None),
        read => read.map(Some),
    }
}
