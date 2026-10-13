//! Bounded JSON retention shared by nextest and focused Playwright checks.
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const TOTAL: usize = 16 * 1024 * 1024;
const LINE: usize = 1024 * 1024;

/// Retain each check's logs in a new, randomly named directory, outside Git.
pub(crate) fn run_directory(cache: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(cache)?;
    Ok(tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(cache)?
        .keep())
}

/// Remove retained `run-*` directories except the `keep` newest.
pub(crate) fn prune(cache: &Path, keep: usize) -> io::Result<()> {
    let mut runs = Vec::new();
    let entries = match fs::read_dir(cache) {
        Ok(entries) => entries,
        // No duel has run in this checkout yet: nothing to prune.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("run-") && entry.file_type()?.is_dir() {
            runs.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    runs.sort();
    let excess = runs.len().saturating_sub(keep);
    for (_, path) in runs.into_iter().take(excess) {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub(crate) struct JsonOutput {
    file: fs::File,
    label: &'static str,
    total: usize,
    line: usize,
}
impl JsonOutput {
    pub(crate) fn create(path: &Path, label: &'static str) -> io::Result<Self> {
        Ok(Self {
            file: fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?,
            label,
            total: 0,
            line: 0,
        })
    }
}
impl Write for JsonOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.total + bytes.len() > TOTAL {
            return Err(io::Error::other(format!(
                "{} JSON total limit exceeded (16 MiB)",
                self.label
            )));
        }
        let mut line = self.line;
        for &byte in bytes {
            if byte == b'\n' {
                line = 0;
            } else {
                line += 1;
            }
            if line > LINE {
                return Err(io::Error::other(format!(
                    "{} JSON line limit exceeded (1 MiB)",
                    self.label
                )));
            }
        }
        self.file.write_all(bytes)?;
        self.total += bytes.len();
        self.line = line;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub(super) fn read(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > TOTAL as u64 {
        return Err(format!("{label} JSON total limit exceeded (16 MiB)"));
    }
    let mut bytes = Vec::new();
    file.take(TOTAL as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > TOTAL {
        return Err(format!("{label} JSON total limit exceeded (16 MiB)"));
    }
    if bytes.split(|b| *b == b'\n').any(|line| line.len() > LINE) {
        return Err(format!("{label} JSON line limit exceeded (1 MiB)"));
    }
    Ok(bytes)
}
