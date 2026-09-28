//! Scratch is owned by a server/worker lease, never by a process ID alone.
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 4096;
const PREFIX: &str = "job-";

pub(super) struct Scratch {
    pub(super) root: PathBuf,
    _lease: File,
}

fn open_lock(path: &Path) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(io::Error::other("scratch lease is not a regular file"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn private_directory(path: &Path, recursive: bool) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn directory(cache: &Path) -> io::Result<(PathBuf, File)> {
    let root = cache.join("worker-scratch");
    private_directory(&root, true)?;
    if !fs::symlink_metadata(&root)?.file_type().is_dir() {
        return Err(io::Error::other("worker scratch is not a plain directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    let lock = open_lock(&root.join(".registry"))?;
    lock.lock()?;
    Ok((root, lock))
}

pub(crate) fn recover(cache: &Path) -> Result<(), String> {
    let (root, _registry) = directory(cache).map_err(|error| error.to_string())?;
    sweep(&root).map_err(|error| format!("could not recover worker scratch: {error}"))
}

fn sweep(root: &Path) -> io::Result<()> {
    for (index, entry) in fs::read_dir(root)?.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(io::Error::other("worker scratch exceeds the entry bound"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(PREFIX) || !entry.file_type()?.is_dir() {
            continue;
        }
        let lease = open_lock(&entry.path().join("lease"))?;
        match lease.try_lock() {
            Ok(()) => fs::remove_dir_all(entry.path())?,
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    Ok(())
}

impl Scratch {
    pub(super) fn new(cache: &Path) -> Result<Self, String> {
        let (root, _registry) = directory(cache).map_err(|error| error.to_string())?;
        sweep(&root).map_err(|error| error.to_string())?;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..32 {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("{PREFIX}{}-{nanos}-{serial}", std::process::id()));
            match private_directory(&path, false) {
                Ok(()) => {
                    let lease =
                        open_lock(&path.join("lease")).map_err(|error| error.to_string())?;
                    lease.lock_shared().map_err(|error| error.to_string())?;
                    return Ok(Self {
                        root: path,
                        _lease: lease,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not allocate worker scratch".to_owned())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // The caller retains this scope until execute has reaped the whole
        // worker group. A crash releases the lease for the next recovery pass.
        let cleanup = || -> io::Result<()> {
            let parent = self
                .root
                .parent()
                .ok_or_else(|| io::Error::other("scratch parent is missing"))?;
            // Serialize removal with directory enumeration and allocation. A
            // concurrent recovery must not reopen a lease after its job scope
            // has disappeared between read_dir and open.
            let registry = open_lock(&parent.join(".registry"))?;
            registry.lock()?;
            fs::remove_dir_all(&self.root)
        };
        if let Err(error) = cleanup() {
            tracing::warn!(%error, path = %self.root.display(), "worker scratch cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests;
