use super::{MAX_BYTES, Result};
#[cfg(unix)]
use nix::{
    fcntl::{Flock, FlockArg, OFlag, open, openat, renameat},
    sys::stat::{Mode, mkdirat},
};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
pub(super) struct Store {
    parent: fs::File,
    root: fs::File,
    lock: Flock<fs::File>,
    directory: PathBuf,
    parent_path: PathBuf,
    uid: u32,
}
#[cfg(unix)]
type Identity = (u64, u64, u32, u32, u64, u64, i64, i64, i64, i64);
#[cfg(unix)]
fn identity(value: &fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    (
        value.dev(),
        value.ino(),
        value.uid(),
        value.mode(),
        value.nlink(),
        value.len(),
        value.mtime(),
        value.mtime_nsec(),
        value.ctime(),
        value.ctime_nsec(),
    )
}
#[cfg(unix)]
fn leaf(value: &fs::Metadata, uid: u32) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !value.is_file()
        || value.uid() != uid
        || value.mode() & 0o7777 != 0o600
        || value.nlink() != 1
    {
        return Err("model journal leaf must be owner0600 single-linked regular file".into());
    }
    Ok(())
}
#[cfg(unix)]
fn directory(path: &Path, handle: &fs::File, uid: u32) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute() {
        return Err("model journal directory must be absolute".into());
    }
    let mut cursor = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::CurDir | Component::ParentDir) {
            return Err("model journal directory must be normal".into());
        }
        cursor.push(component);
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err("model journal ancestor is linked".into());
        }
    }
    let observed = fs::symlink_metadata(path)?;
    let opened = handle.metadata()?;
    if !observed.is_dir()
        || !opened.is_dir()
        || observed.uid() != uid
        || observed.mode() & 0o7777 != 0o700
        || observed.dev() != opened.dev()
        || observed.ino() != opened.ino()
        || opened.uid() != uid
        || opened.mode() & 0o7777 != 0o700
    {
        return Err("model journal directory owner/mode/handle identity changed".into());
    }
    Ok(())
}
#[cfg(unix)]
impl Store {
    pub(super) fn open(parent_path: &Path) -> Result<Self> {
        let uid = nix::unistd::geteuid().as_raw();
        let parent: fs::File = open(
            parent_path,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?
        .into();
        directory(parent_path, &parent, uid)?;
        match mkdirat(&parent, "lease-journal", Mode::S_IRWXU) {
            Ok(()) => parent.sync_all()?,
            Err(nix::errno::Errno::EEXIST) => (),
            Err(error) => return Err(error.into()),
        }
        let root: fs::File = openat(
            &parent,
            "lease-journal",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?
        .into();
        let directory_path = parent_path.join("lease-journal");
        directory(&directory_path, &root, uid)?;
        let lock_file: fs::File = openat(
            &root,
            "lock",
            OFlag::O_RDWR
                | OFlag::O_CREAT
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK
                | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into();
        leaf(&lock_file.metadata()?, uid)?;
        let lock =
            Flock::lock(lock_file, FlockArg::LockExclusiveNonblock).map_err(|(_, error)| error)?;
        lock.sync_all()?;
        root.sync_all()?;
        let store = Self {
            parent,
            root,
            lock,
            directory: directory_path,
            parent_path: parent_path.to_owned(),
            uid,
        };
        store.check()?;
        Ok(store)
    }
    pub(super) fn uid(&self) -> u32 {
        self.uid
    }
    fn check(&self) -> Result<()> {
        directory(&self.parent_path, &self.parent, self.uid)?;
        directory(&self.directory, &self.root, self.uid)?;
        let opened = self.lock.metadata()?;
        let path = fs::symlink_metadata(self.directory.join("lock"))?;
        leaf(&opened, self.uid)?;
        leaf(&path, self.uid)?;
        if identity(&opened) != identity(&path) {
            return Err("model journal lock identity changed".into());
        }
        Ok(())
    }
    pub(super) fn read(&self) -> Result<Option<Vec<u8>>> {
        self.check()?;
        let path = self.directory.join("journal.json");
        let before = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        leaf(&before, self.uid)?;
        let mut file: fs::File = openat(
            &self.root,
            "journal.json",
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?
        .into();
        let opened = file.metadata()?;
        leaf(&opened, self.uid)?;
        if identity(&before) != identity(&opened) {
            return Err("model journal identity changed before read".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err("model journal exceeds32KiB".into());
        }
        file.seek(SeekFrom::Start(0))?;
        let mut endpoint = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut endpoint)?;
        self.check()?;
        let end_handle = file.metadata()?;
        let end_path = fs::symlink_metadata(&path)?;
        leaf(&end_handle, self.uid)?;
        leaf(&end_path, self.uid)?;
        if bytes != endpoint
            || identity(&opened) != identity(&end_handle)
            || identity(&opened) != identity(&end_path)
        {
            return Err("model journal endpoint changed during read".into());
        }
        Ok(Some(bytes))
    }
    pub(super) fn replace(&self, expected: Option<&[u8]>, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_BYTES {
            return Err("model journal write exceeds32KiB".into());
        }
        if self.read()?.as_deref() != expected {
            return Err("model journal changed before replacement".into());
        }
        let name = format!(
            ".model-journal-{}-{}.tmp",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let mut file: fs::File = openat(
            &self.root,
            name.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )?
        .into();
        leaf(&file.metadata()?, self.uid)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        self.check()?;
        if self.read()?.as_deref() != expected {
            return Err("model journal changed before rename; private temporary retained".into());
        }
        renameat(&self.root, name.as_str(), &self.root, "journal.json")?;
        self.root.sync_all()?;
        self.check()?;
        if self.read()?.as_deref() != Some(bytes) {
            return Err("model journal replacement endpoint changed".into());
        }
        Ok(())
    }
}
#[cfg(not(unix))]
pub(super) struct Store;
#[cfg(not(unix))]
impl Store {
    pub(super) fn open(_: &Path) -> Result<Self> {
        Err("model journal requires Unix no-follow ownership support".into())
    }
    pub(super) fn uid(&self) -> u32 {
        0
    }
    pub(super) fn read(&self) -> Result<Option<Vec<u8>>> {
        Err("model journal unavailable".into())
    }
    pub(super) fn replace(&self, _: Option<&[u8]>, _: &[u8]) -> Result<()> {
        Err("model journal unavailable".into())
    }
}
