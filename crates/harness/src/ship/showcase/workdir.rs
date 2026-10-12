use std::{
    fs,
    io::Write,
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

/// The recorder and the module it imports, run by node in the work directory.
const RECORDER: [(&str, &str); 2] = [
    ("record.mjs", include_str!("record.mjs")),
    ("scenes.mjs", include_str!("scenes.mjs")),
];

pub(crate) fn write_recorder(work: &Path) -> std::io::Result<()> {
    for (name, text) in RECORDER {
        create_new(&work.join(name))?.write_all(text.as_bytes())?;
    }
    Ok(())
}
