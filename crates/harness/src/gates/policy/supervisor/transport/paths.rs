//! Fixed client configuration/endpoint metadata observations, not trust admission.
use super::*;
use std::{
    fs,
    io::Read,
    path::{Component, PathBuf},
};
#[cfg(unix)]
type Identity = (u64, u64, u32, u32, u64, u64, i64, i64, i64, i64);
#[derive(PartialEq, Eq)]
pub(super) struct Observed {
    #[cfg(unix)]
    program: Identity,
    #[cfg(unix)]
    socket: Identity,
    #[cfg(unix)]
    config: Identity,
    #[cfg(unix)]
    config_file: Option<(Identity, Vec<u8>)>,
}
pub(super) fn fixed() -> Result<Observed, &'static str> {
    #[cfg(unix)]
    {
        inspect(
            Path::new(PROGRAM),
            Path::new(SOCKET),
            Path::new(CONFIG),
            0,
            Path::new("/"),
        )
    }
    #[cfg(not(unix))]
    {
        Err("daemon transport requires Unix socket and ownership observations")
    }
}
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
fn ancestors(path: &Path, uid: u32, boundary: &Path) -> Result<(), &'static str> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute()
        || !path.starts_with(boundary)
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err("fixed transport path is not normal absolute path");
    }
    let parent = path.parent().ok_or("fixed transport parent unavailable")?;
    let mut cursor = PathBuf::from(boundary);
    let check = |cursor: &Path| -> Result<(), &'static str> {
        let metadata =
            fs::symlink_metadata(cursor).map_err(|_| "fixed transport ancestor unavailable")?;
        if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
            return Err("fixed transport ancestor linked, wrong owner or writable by others");
        }
        Ok(())
    };
    check(&cursor)?;
    for part in parent
        .strip_prefix(boundary)
        .map_err(|_| "fixed transport boundary escaped")?
        .components()
    {
        if !matches!(part, Component::Normal(_)) {
            return Err("fixed transport ancestor not normal");
        }
        cursor.push(part);
        check(&cursor)?;
    }
    Ok(())
}
#[cfg(unix)]
fn config_file(
    path: &Path,
    uid: u32,
    boundary: &Path,
) -> Result<(Identity, Vec<u8>), &'static str> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    ancestors(path, uid, boundary)?;
    let before =
        fs::symlink_metadata(path).map_err(|_| "fixed Docker client config unavailable")?;
    if !before.is_file()
        || before.uid() != uid
        || before.mode() & 0o7777 != 0o600
        || before.nlink() != 1
    {
        return Err("fixed Docker config must be owner0600 single-linked regular file");
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "fixed Docker config no-follow read unavailable")?;
    let opened = file
        .metadata()
        .map_err(|_| "fixed Docker config handle unavailable")?;
    if identity(&opened) != identity(&before) {
        return Err("fixed Docker config changed before open");
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "fixed Docker config read unavailable")?;
    if bytes.len() > 4096 || serde_json::from_slice::<Value>(&bytes).ok() != Some(json!({})) {
        return Err(
            "fixed Docker config must be bounded empty JSON object; no credentials/helpers/contexts",
        );
    }
    let after =
        fs::symlink_metadata(path).map_err(|_| "fixed Docker config endpoint unavailable")?;
    let end = file
        .metadata()
        .map_err(|_| "fixed Docker config endpoint handle unavailable")?;
    if identity(&after) != identity(&before) || identity(&end) != identity(&before) {
        return Err("fixed Docker config changed during observation");
    }
    Ok((identity(&end), bytes))
}
// Private test primitive. Production uses ONLY fixed literals, UID0 and root boundary.
#[cfg(unix)]
pub(super) fn inspect(
    program: &Path,
    socket: &Path,
    config: &Path,
    uid: u32,
    boundary: &Path,
) -> Result<Observed, &'static str> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    for path in [program, socket, config] {
        ancestors(path, uid, boundary)?;
    }
    let p = fs::symlink_metadata(program).map_err(|_| "fixed Docker executable unavailable")?;
    if !p.is_file()
        || p.uid() != uid
        || p.mode() & 0o022 != 0
        || p.mode() & 0o7000 != 0
        || p.mode() & 0o100 == 0
        || p.nlink() != 1
    {
        return Err("fixed Docker executable linked, nonregular or insufficiently protected");
    }
    let s = fs::symlink_metadata(socket).map_err(|_| "fixed service Docker socket unavailable")?;
    if !s.file_type().is_socket() || s.uid() != uid || s.mode() & 0o7777 != 0o600 || s.nlink() != 1
    {
        return Err("fixed service Docker socket must be owner0600 single-linked socket");
    }
    let c =
        fs::symlink_metadata(config).map_err(|_| "fixed Docker client directory unavailable")?;
    if !c.is_dir() || c.uid() != uid || c.mode() & 0o022 != 0 {
        return Err("fixed Docker client directory linked, wrong owner or writable by others");
    }
    let mut found = None;
    for entry in fs::read_dir(config).map_err(|_| "fixed Docker client directory unreadable")? {
        let entry = entry.map_err(|_| "fixed Docker client directory entry unavailable")?;
        if entry.file_name() != "config.json" || found.is_some() {
            return Err("fixed Docker client directory contains unapproved files");
        }
        found = Some(config_file(&entry.path(), uid, boundary)?);
    }
    Ok(Observed {
        program: identity(&p),
        socket: identity(&s),
        config: identity(&c),
        config_file: found,
    })
}
