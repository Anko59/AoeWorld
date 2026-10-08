use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub(super) fn create_new(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)
}

pub(crate) fn create_work_dir(
    root: &Path,
) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let tmp = super::showcase_tmp(root)?;
    let work = tempfile::Builder::new()
        .prefix("showcase-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(tmp)?;
    let directory = super::open_directory(work.path())?;
    nix::sys::stat::fchmod(&directory, nix::sys::stat::Mode::S_IRWXU)?;
    Ok(work)
}
