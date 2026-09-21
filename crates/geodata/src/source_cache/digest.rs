use super::{COPY_BUFFER_BYTES, CacheError};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

pub(super) type FileHashes = ([u8; 32], [u8; 20], [u8; 16]);

pub(super) fn file_hashes(path: &Path) -> Result<FileHashes, CacheError> {
    let mut file = File::open(path)?;
    let mut sha256 = Sha256::new();
    let mut sha1 = Sha1::new();
    let mut md5 = Md5::new();
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha256.update(&buffer[..count]);
        sha1.update(&buffer[..count]);
        md5.update(&buffer[..count]);
    }
    Ok((
        sha256.finalize().into(),
        sha1.finalize().into(),
        md5.finalize().into(),
    ))
}

pub(super) fn digest_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
