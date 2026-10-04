use super::*;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    schema: u32,
    repository_id: u64,
    public_key: String,
    key_id: String,
}
pub(super) struct Observed {
    pub(super) repository_id: u64,
    pub(super) public_key: [u8; 32],
    pub(super) key_id: String,
    pub(super) file_blake3: String,
}
pub(super) fn fixed() -> Result<Observed, &'static str> {
    #[cfg(unix)]
    {
        read(Path::new(KEY_PATH), 0, Path::new("/"))
    }
    #[cfg(not(unix))]
    {
        Err("root-key observation requires Unix ownership and no-follow support")
    }
}
#[cfg(unix)]
fn identity(metadata: &fs::Metadata) -> (u64, u64, u32, u32, u64, u64, i64, i64, i64, i64) {
    use std::os::unix::fs::MetadataExt;
    (
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.mode(),
        metadata.nlink(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}
#[cfg(unix)]
fn check_leaf(metadata: &fs::Metadata, uid: u32) -> Result<(), &'static str> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
    {
        return Err("root-key leaf is not single-linked owner0600 regular file");
    }
    Ok(())
}
#[cfg(unix)]
fn ancestors(path: &Path, uid: u32, boundary: &Path) -> Result<(), &'static str> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute()
        || !path.starts_with(boundary)
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("root-key path is not normal absolute path");
    }
    let parent = path.parent().ok_or("root-key parent absent")?;
    let mut cursor = PathBuf::from(boundary);
    let check = |value: &Path| -> Result<(), &'static str> {
        let m = fs::symlink_metadata(value).map_err(|_| "root-key ancestor unavailable")?;
        if !m.is_dir() || m.uid() != uid || m.mode() & 0o022 != 0 {
            return Err("root-key ancestor is linked, wrong owner or writable by others");
        }
        Ok(())
    };
    check(&cursor)?;
    for component in parent
        .strip_prefix(boundary)
        .map_err(|_| "root-key escaped observation boundary")?
        .components()
    {
        if !matches!(component, Component::Normal(_)) {
            return Err("root-key ancestor non-normal");
        }
        cursor.push(component);
        check(&cursor)?;
    }
    Ok(())
}
#[cfg(unix)]
fn bytes(file: &mut fs::File) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    file.by_ref()
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "root-key read unavailable")?;
    if bytes.len() > 4096 {
        return Err("root-key exceeds4096 bytes");
    }
    Ok(bytes)
}
// Private observation seam; production supplies only fixed path/root UID/boundary.
// Unit fixtures use a private temporary boundary, never a public CLI override.
#[cfg(unix)]
pub(super) fn read(path: &Path, uid: u32, boundary: &Path) -> Result<Observed, &'static str> {
    use std::os::unix::fs::OpenOptionsExt;
    ancestors(path, uid, boundary)?;
    let initial = fs::symlink_metadata(path).map_err(|_| "fixed root-key unavailable")?;
    check_leaf(&initial, uid)?;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "root-key no-follow open unavailable")?;
    let opened = file
        .metadata()
        .map_err(|_| "root-key handle metadata unavailable")?;
    check_leaf(&opened, uid)?;
    if identity(&initial) != identity(&opened) {
        return Err("root-key identity changed before open");
    }
    let raw = bytes(&mut file)?;
    let value: KeyFile =
        serde_json::from_slice(&raw).map_err(|_| "root-key strict JSON invalid")?;
    let public_key = hex::<32>(&value.public_key).map_err(|_| "root-key public bytes invalid")?;
    if value.schema != 1 || value.repository_id == 0 || value.key_id != key_id(&public_key) {
        return Err("root-key schema/repository/self identity invalid");
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "root-key endpoint seek unavailable")?;
    let final_bytes = bytes(&mut file)?;
    let final_handle = file
        .metadata()
        .map_err(|_| "root-key endpoint handle unavailable")?;
    ancestors(path, uid, boundary)?;
    let final_path =
        fs::symlink_metadata(path).map_err(|_| "root-key endpoint path unavailable")?;
    check_leaf(&final_handle, uid)?;
    check_leaf(&final_path, uid)?;
    if raw != final_bytes
        || identity(&opened) != identity(&final_handle)
        || identity(&opened) != identity(&final_path)
    {
        return Err("root-key bytes or identity changed");
    }
    Ok(Observed {
        repository_id: value.repository_id,
        public_key,
        key_id: value.key_id,
        file_blake3: blake3::hash(&raw).to_hex().to_string(),
    })
}
