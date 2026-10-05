use super::Result;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    if path
        .as_os_str()
        .as_encoded_bytes()
        .split(|byte| *byte == std::path::MAIN_SEPARATOR as u8)
        .any(|part| part == b"." || part == b"..")
        || path.components().any(|part| {
            matches!(
                part,
                Component::CurDir | Component::ParentDir | Component::Prefix(_)
            )
        })
    {
        return Err("QA paths require normal components".into());
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    // Normalize repeated separators without resolving any filesystem symlinks.
    Ok(path.components().collect())
}
fn normal(path: &Path, directory: bool) -> Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink()
            || ((current != path || directory) && !metadata.is_dir())
        {
            return Err("QA paths cannot contain symlinks or non-directory ancestors".into());
        }
    }
    Ok(())
}
pub(super) fn root(path: &Path) -> Result<PathBuf> {
    let path = absolute(path)?;
    normal(&path, true)?;
    Ok(path)
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
fn identity(metadata: &Metadata, limit: u64) -> Result<Identity> {
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > limit {
        return Err("QA input must be a bounded single-link regular file".into());
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
fn identity(_: &Metadata, _: u64) -> Result<Identity> {
    Err("QA held-file observations require Unix identity checks".into())
}

pub(super) struct Held {
    path: PathBuf,
    file: File,
    identity: Identity,
    limit: u64,
}
impl Held {
    pub(super) fn open(path: &Path, limit: u64) -> Result<Self> {
        let path = absolute(path)?;
        normal(&path, false)?;
        let before = identity(&fs::symlink_metadata(&path)?, limit)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
        let file = options.open(&path)?;
        if identity(&file.metadata()?, limit)? != before {
            return Err("QA input changed while opening".into());
        }
        Ok(Self {
            path,
            file,
            identity: before,
            limit,
        })
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn same_file(&self, other: &Self) -> bool {
        self.identity.dev == other.identity.dev && self.identity.ino == other.identity.ino
    }
    pub(super) fn length(&self) -> u64 {
        self.identity.len
    }
    pub(super) fn check(&self) -> Result<()> {
        normal(&self.path, false)?;
        if identity(&fs::symlink_metadata(&self.path)?, self.limit)? != self.identity
            || identity(&self.file.metadata()?, self.limit)? != self.identity
        {
            return Err("QA input changed during observation".into());
        }
        Ok(())
    }
    pub(super) fn read(&mut self) -> Result<Vec<u8>> {
        self.check()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        (&mut self.file)
            .take(self.identity.len + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != self.identity.len {
            return Err("QA input length changed during observation".into());
        }
        self.check()?;
        Ok(bytes)
    }
    pub(super) fn digest(&mut self) -> Result<String> {
        Ok(blake3::hash(&self.read()?).to_hex().to_string())
    }
    pub(super) fn recheck(&mut self, expected: &str) -> Result<()> {
        if self.digest()? != expected {
            return Err("QA input bytes changed during observation".into());
        }
        Ok(())
    }
}
