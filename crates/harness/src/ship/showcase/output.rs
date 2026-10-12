//! Where a showcase run reads and writes: the validated output file and the
//! private temporary parent, opened by descriptor so a replaced path component
//! cannot redirect a write.
use std::{
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

/// The validated output parent held open for the complete showcase run.
/// Publication uses only this directory descriptor, so replacing a path
/// component after validation cannot redirect the final write.
pub(crate) struct ShowcaseOutput {
    pub(crate) directory: std::fs::File,
    pub(crate) name: std::ffi::CString,
    pub(crate) path: std::path::PathBuf,
}

/// Resolve the requested video path beneath `.cache/showcase`, refusing
/// symlinks in every component and multiply-linked existing files.
pub(crate) fn resolve_out(root: &Path, requested: &str) -> Result<ShowcaseOutput, String> {
    let requested_path = Path::new(requested);
    if !requested_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".webm"))
    {
        return Err("SHOWCASE_OUT must end in `.webm`".into());
    }
    if !super::super::github::attachable_video(requested_path) {
        return Err("SHOWCASE_OUT filename must use only letters, digits, `.`, `_` or `-`".into());
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let root_fd =
        open_directory(&root).map_err(|error| format!("cannot open repository root: {error}"))?;
    let cache_fd = open_or_create_child(&root_fd, ".cache")
        .map_err(|error| format!("cannot open .cache: {error}"))?;
    let base_fd = open_or_create_child(&cache_fd, "showcase")
        .map_err(|error| format!("cannot open .cache/showcase: {error}"))?;
    let base = root.join(".cache/showcase");
    let requested = Path::new(requested);
    if requested
        .components()
        .any(|part| part == Component::ParentDir)
    {
        return Err("SHOWCASE_OUT must not contain `..`".into());
    }
    let relative = if requested.is_absolute() {
        requested
            .strip_prefix(&base)
            .map_err(|_| "SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`")?
            .to_path_buf()
    } else {
        let parts: Vec<_> = requested.components().collect();
        if parts.starts_with(&[
            Component::Normal(std::ffi::OsStr::new(".cache")),
            Component::Normal(std::ffi::OsStr::new("showcase")),
        ]) {
            requested
                .strip_prefix(Path::new(".cache/showcase"))
                .map_err(|_| "SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`")?
                .to_path_buf()
        } else {
            return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into());
        }
    };
    if relative.as_os_str().is_empty() {
        return Err("SHOWCASE_OUT must name a file inside `<repo>/.cache/showcase/`".into());
    }
    let mut parent_path = base.clone();
    let mut parent_fd = base_fd;
    let mut file = None;
    let parts: Vec<_> = relative.components().collect();
    for (index, part) in parts.iter().enumerate() {
        match part {
            Component::Normal(name) if index + 1 == parts.len() => file = Some(name),
            Component::Normal(name) => {
                let component = name.to_str().ok_or("SHOWCASE_OUT path must be UTF-8")?;
                parent_fd = open_or_create_child(&parent_fd, component)
                    .map_err(|error| format!("cannot open output directory: {error}"))?;
                parent_path.push(name);
            }
            Component::CurDir => {}
            _ => return Err("SHOWCASE_OUT must resolve inside `<repo>/.cache/showcase/`".into()),
        }
    }
    let file = file.ok_or("SHOWCASE_OUT must name a file inside `<repo>/.cache/showcase/`")?;
    let name = std::ffi::CString::new(file.as_bytes())
        .map_err(|_| "SHOWCASE_OUT filename contains a NUL byte")?;
    match nix::sys::stat::fstatat(
        &parent_fd,
        name.as_c_str(),
        nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
    ) {
        Ok(metadata)
            if metadata.st_mode & nix::libc::S_IFMT != nix::libc::S_IFREG
                || metadata.st_nlink > 1 =>
        {
            return Err(
                "SHOWCASE_OUT must be a single-link regular file, not a symlink or directory"
                    .into(),
            );
        }
        Ok(_) => {}
        Err(nix::errno::Errno::ENOENT) => {}
        Err(error) => return Err(format!("cannot inspect SHOWCASE_OUT: {error}")),
    }
    Ok(ShowcaseOutput {
        directory: parent_fd,
        name,
        path: parent_path.join(file),
    })
}

/// Create and open the private temporary parent used by showcase runs.
pub(crate) fn showcase_tmp(root: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let root_fd =
        open_directory(&root).map_err(|error| format!("cannot open repository root: {error}"))?;
    let cache_fd = open_or_create_child(&root_fd, ".cache")
        .map_err(|error| format!("cannot open .cache: {error}"))?;
    let tmp_fd = open_or_create_private_child(&cache_fd, "tmp")
        .map_err(|error| format!("cannot open .cache/tmp: {error}"))?;
    nix::sys::stat::fchmod(&tmp_fd, nix::sys::stat::Mode::S_IRWXU)
        .map_err(|error| format!("cannot secure .cache/tmp: {error}"))?;
    let stat = nix::sys::stat::fstat(&tmp_fd)
        .map_err(|error| format!("cannot inspect .cache/tmp: {error}"))?;
    if stat.st_uid != nix::unistd::getuid().as_raw() {
        return Err(".cache/tmp must be owned by the current user".into());
    }
    Ok(root.join(".cache/tmp"))
}

pub(super) fn open_directory(path: &Path) -> nix::Result<std::fs::File> {
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )?;
    Ok(std::fs::File::from(fd))
}

fn open_or_create_child(parent: &std::fs::File, name: &str) -> nix::Result<std::fs::File> {
    let name = std::ffi::CString::new(name).map_err(|_| nix::errno::Errno::EINVAL)?;
    match nix::fcntl::openat(
        parent,
        name.as_c_str(),
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => Ok(std::fs::File::from(fd)),
        Err(nix::errno::Errno::ENOENT) => {
            match nix::sys::stat::mkdirat(
                parent,
                name.as_c_str(),
                nix::sys::stat::Mode::S_IRWXU
                    | nix::sys::stat::Mode::S_IRGRP
                    | nix::sys::stat::Mode::S_IXGRP
                    | nix::sys::stat::Mode::S_IROTH
                    | nix::sys::stat::Mode::S_IXOTH,
            ) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(error) => return Err(error),
            }
            let fd = nix::fcntl::openat(
                parent,
                name.as_c_str(),
                nix::fcntl::OFlag::O_RDONLY
                    | nix::fcntl::OFlag::O_DIRECTORY
                    | nix::fcntl::OFlag::O_NOFOLLOW
                    | nix::fcntl::OFlag::O_CLOEXEC,
                nix::sys::stat::Mode::empty(),
            )?;
            Ok(std::fs::File::from(fd))
        }
        Err(error) => Err(error),
    }
}

fn open_or_create_private_child(parent: &std::fs::File, name: &str) -> nix::Result<std::fs::File> {
    let name = std::ffi::CString::new(name).map_err(|_| nix::errno::Errno::EINVAL)?;
    let flags = nix::fcntl::OFlag::O_RDONLY
        | nix::fcntl::OFlag::O_DIRECTORY
        | nix::fcntl::OFlag::O_NOFOLLOW
        | nix::fcntl::OFlag::O_CLOEXEC;
    match nix::fcntl::openat(
        parent,
        name.as_c_str(),
        flags,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => Ok(std::fs::File::from(fd)),
        Err(nix::errno::Errno::ENOENT) => {
            match nix::sys::stat::mkdirat(parent, name.as_c_str(), nix::sys::stat::Mode::S_IRWXU) {
                Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                Err(error) => return Err(error),
            }
            let fd = nix::fcntl::openat(
                parent,
                name.as_c_str(),
                flags,
                nix::sys::stat::Mode::empty(),
            )?;
            Ok(std::fs::File::from(fd))
        }
        Err(error) => Err(error),
    }
}
