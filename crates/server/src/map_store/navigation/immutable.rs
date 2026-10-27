//! Safe mappings require kernel-enforced immutability. Ordinary cache files
//! are copied in bounded buffers into a sealed memfd, never into a heap blob.
use super::*;
use memmap2::Mmap;

pub(super) fn map(
    file: &mut fs::File,
    length: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Mmap, String> {
    match Mmap::map_if_safe(&*file) {
        Ok(mapping) => Ok(mapping),
        Err(error) if error.not_safe() || error.not_supported() => sealed(file, length, cancelled),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn sealed(
    source: &mut fs::File,
    length: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<Mmap, String> {
    use nix::{
        fcntl::{FcntlArg, SealFlag, fcntl},
        sys::memfd::{MFdFlags, memfd_create},
    };
    use std::io::{Read, Seek, SeekFrom, Write};
    let fd = memfd_create(
        "aoe-navigation",
        MFdFlags::MFD_CLOEXEC | MFdFlags::MFD_ALLOW_SEALING,
    )
    .map_err(|e| e.to_string())?;
    let mut target = fs::File::from(fd);
    source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1 << 20];
    let mut left = length;
    while left > 0 {
        if cancelled() {
            return Err("navigation load cancelled".into());
        }
        let take = left.min(buffer.len() as u64) as usize;
        source
            .read_exact(&mut buffer[..take])
            .map_err(|e| e.to_string())?;
        target
            .write_all(&buffer[..take])
            .map_err(|e| e.to_string())?;
        left -= take as u64;
    }
    fcntl(
        &target,
        FcntlArg::F_ADD_SEALS(
            SealFlag::F_SEAL_WRITE
                | SealFlag::F_SEAL_SHRINK
                | SealFlag::F_SEAL_GROW
                | SealFlag::F_SEAL_SEAL,
        ),
    )
    .map_err(|e| e.to_string())?;
    Mmap::map_if_safe(&target).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
fn sealed(_: &mut fs::File, _: u64, _: &dyn Fn() -> bool) -> Result<Mmap, String> {
    Err("safe navigation mapping requires fs-verity or Linux sealed memfd support".into())
}
