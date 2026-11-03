//! Shared container caps and immediate memory admission; no slots or load wait.
use std::{fs, io};

pub(crate) const FLOOR_KIB: u64 = 3 * 1024 * 1024;
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
    admit(&fs::read_to_string("/proc/meminfo")?)
}
