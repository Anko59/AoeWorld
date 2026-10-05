//! Bounded original-FD observations of the two fixed candidate artifacts.
use super::Result;
use serde::Serialize;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};
const LIMIT: u64 = 4 * 1024 * 1024;

pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    if path == Path::new(".") {
        return Ok(std::env::current_dir()?);
    }
    if path
        .as_os_str()
        .as_encoded_bytes()
        .split(|byte| *byte == b'/')
        .any(|part| part == b"." || part == b"..")
        || path.components().any(|part| {
            matches!(
                part,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err("mutation paths must have normal components".into());
    }
    Ok(if path.is_absolute() {
        path.components().collect()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn normal(path: &Path, directory: bool) -> Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink()
            || ((directory || current != path) && !metadata.is_dir())
        {
            return Err("mutation ancestor or path is not normal".into());
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Identity {
    dev: u64,
    ino: u64,
    mode: u32,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
#[cfg(unix)]
fn identity(metadata: &Metadata) -> Result<Identity> {
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > LIMIT {
        return Err("mutation artifacts must be bounded single-link regular files".into());
    }
    Ok(Identity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        len: metadata.len(),
        mtime: (metadata.mtime(), metadata.mtime_nsec()),
        ctime: (metadata.ctime(), metadata.ctime_nsec()),
    })
}
#[cfg(not(unix))]
fn identity(_: &Metadata) -> Result<Identity> {
    Err("mutation observations require Unix identities".into())
}
#[derive(Eq, PartialEq)]
struct DirectoryIdentity {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
}
#[cfg(unix)]
fn directory_identity(metadata: &Metadata) -> Result<DirectoryIdentity> {
    if !metadata.is_dir() {
        return Err("mutation ancestor is not a directory".into());
    }
    Ok(DirectoryIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        uid: metadata.uid(),
        gid: metadata.gid(),
    })
}
#[cfg(not(unix))]
fn directory_identity(_: &Metadata) -> Result<DirectoryIdentity> {
    Err("mutation observations require Unix identities".into())
}
struct Directory {
    path: PathBuf,
    file: File,
    identity: DirectoryIdentity,
}
impl Directory {
    fn open(path: &Path) -> Result<Self> {
        let before = directory_identity(&fs::symlink_metadata(path)?)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK | nix::libc::O_DIRECTORY);
        let file = options.open(path)?;
        if directory_identity(&file.metadata()?)? != before {
            return Err("mutation directory changed opening".into());
        }
        Ok(Self {
            path: path.into(),
            file,
            identity: before,
        })
    }
    fn check(&self) -> Result<()> {
        normal(&self.path, true)?;
        if directory_identity(&fs::symlink_metadata(&self.path)?)? != self.identity
            || directory_identity(&self.file.metadata()?)? != self.identity
        {
            return Err("mutation directory changed".into());
        }
        Ok(())
    }
}
struct Held {
    path: PathBuf,
    file: File,
    identity: Identity,
}
impl Held {
    fn open(path: &Path) -> Result<Self> {
        normal(path, false)?;
        let before = identity(&fs::symlink_metadata(path)?)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        let file = options.open(path)?;
        if identity(&file.metadata()?)? != before {
            return Err("mutation artifact changed opening".into());
        }
        Ok(Self {
            path: path.into(),
            file,
            identity: before,
        })
    }
    fn check(&self) -> Result<()> {
        normal(&self.path, false)?;
        if identity(&fs::symlink_metadata(&self.path)?)? != self.identity
            || identity(&self.file.metadata()?)? != self.identity
        {
            return Err("mutation artifact changed".into());
        }
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>> {
        self.check()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        (&mut self.file)
            .take(self.identity.len + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != self.identity.len {
            return Err("mutation artifact length changed".into());
        }
        self.check()?;
        Ok(bytes)
    }
}
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct Measurement {
    pub(super) bytes: u64,
    pub(super) raw_blake3: String,
}
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct Measurements {
    pub(super) outcomes: Measurement,
    pub(super) inventory: Measurement,
}
pub(super) struct Pair {
    directories: Vec<Directory>,
    outcomes: Held,
    inventory: Held,
    pub(super) outcomes_bytes: Vec<u8>,
    pub(super) inventory_bytes: Vec<u8>,
    pub(super) measurements: Measurements,
}
impl Pair {
    #[cfg(test)]
    pub(super) fn at(root: &Path) -> Result<Self> {
        Self::open(&absolute(root)?.join(super::OUTPUT).join("mutants.out"))
    }
    pub(super) fn open(path: &Path) -> Result<Self> {
        let root = absolute(path)?;
        normal(&root, true)?;
        let mut directories = Vec::new();
        let mut current = PathBuf::new();
        for part in root.components() {
            current.push(part.as_os_str());
            directories.push(Directory::open(&current)?);
        }
        let mut outcomes = Held::open(&root.join("outcomes.json"))?;
        let mut inventory = Held::open(&root.join("mutants.json"))?;
        if outcomes.identity.dev == inventory.identity.dev
            && outcomes.identity.ino == inventory.identity.ino
        {
            return Err("mutation artifacts alias".into());
        }
        let outcomes_bytes = outcomes.read()?;
        let inventory_bytes = inventory.read()?;
        let measurements = Measurements {
            outcomes: Measurement {
                bytes: outcomes_bytes.len() as u64,
                raw_blake3: blake3::hash(&outcomes_bytes).to_hex().to_string(),
            },
            inventory: Measurement {
                bytes: inventory_bytes.len() as u64,
                raw_blake3: blake3::hash(&inventory_bytes).to_hex().to_string(),
            },
        };
        let mut pair = Self {
            directories,
            outcomes,
            inventory,
            outcomes_bytes,
            inventory_bytes,
            measurements,
        };
        pair.recheck()?;
        Ok(pair)
    }
    pub(super) fn recheck(&mut self) -> Result<()> {
        for directory in &self.directories {
            directory.check()?;
        }
        if blake3::hash(&self.outcomes.read()?).to_hex().to_string()
            != self.measurements.outcomes.raw_blake3
            || blake3::hash(&self.inventory.read()?).to_hex().to_string()
                != self.measurements.inventory.raw_blake3
        {
            return Err("mutation artifact bytes changed".into());
        }
        for directory in &self.directories {
            directory.check()?;
        }
        Ok(())
    }
}

// Local publication remains non-atomic/non-authoritative. Hold verified regular
// FDs and never truncate an unchecked symlink/hardlink/FIFO target.
pub(super) fn write_known(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    if !matches!(name, "nightly.json" | "nightly.md") {
        return Err("mutation publication name invalid".into());
    }
    let directory = absolute(root)?.join("reports/mutation");
    // Check existing ancestors before creation; no symlink-following mkdir chain.
    let mut current = PathBuf::new();
    for part in directory.components() {
        current.push(part.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("mutation publication ancestor invalid".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(error) => return Err(error.into()),
        }
    }
    normal(&directory, true)?;
    let held_directory = Directory::open(&directory)?;
    let path = directory.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        identity(&metadata)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    options
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .mode(0o600);
    let mut file = options.open(&path)?;
    let held = identity(&file.metadata()?)?;
    if identity(&fs::symlink_metadata(&path)?)? != held {
        return Err("mutation publication changed opening".into());
    }
    held_directory.check()?;
    file.set_len(0)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    held_directory.check()?;
    if identity(&file.metadata()?)? != identity(&fs::symlink_metadata(&path)?)? {
        return Err("mutation publication target changed".into());
    }
    Ok(())
}
