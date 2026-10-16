//! Cache directories and entries are opened without following symlinks. A
//! publication is atomic and private, using the same descriptor-held output
//! writer as the finished video; no failed or partial response is published.
use super::Result;
use crate::ship::showcase::{
    ShowcaseOutput,
    output::{open_directory, open_or_create_child},
};
use std::{ffi::CString, fs, io::Read, os::unix::fs::MetadataExt, path::Path};

pub(super) fn key(model: &str, voice: &str, text: &str) -> String {
    let input = serde_json::json!([model, voice, text, "pcm", super::super::PCM_RATE]).to_string();
    blake3::hash(input.as_bytes()).to_hex().to_string()
}

pub(super) fn entry(root: &Path, model: &str, voice: &str, text: &str) -> Result<ShowcaseOutput> {
    let root = root.canonicalize()?;
    let root_fd = open_directory(&root)?;
    let cache_fd = open_or_create_child(&root_fd, ".cache")?;
    let directory = open_or_create_child(&cache_fd, "showcase-tts")?;
    nix::sys::stat::fchmod(&directory, nix::sys::stat::Mode::S_IRWXU)?;
    let name = format!("{}.pcm", key(model, voice, text));
    Ok(ShowcaseOutput {
        directory,
        path: root.join(".cache/showcase-tts").join(&name),
        name: CString::new(name)?,
    })
}

pub(super) fn load(entry: &ShowcaseOutput, audio: &Path, remaining: u64) -> Result<Option<u64>> {
    use nix::fcntl::{OFlag, openat};
    let fd = match openat(
        &entry.directory,
        entry.name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        nix::sys::stat::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(error) => return Err(format!("cached speech {}: {error}", entry.path.display()).into()),
    };
    let file = fs::File::from(fd);
    let meta = file.metadata()?;
    let bytes = meta.len();
    if !meta.is_file() || meta.nlink() != 1 || bytes == 0 || bytes % 2 != 0 {
        return Err(format!(
            "cached speech {} is not valid single-link PCM; delete it to regenerate",
            entry.path.display()
        )
        .into());
    }
    if bytes > remaining {
        return Err(
            format!("cached speech exceeds the {remaining}-byte aggregate PCM budget").into(),
        );
    }
    let mut target = super::create_new(audio)?;
    if std::io::copy(&mut file.take(bytes), &mut target)? != bytes {
        return Err("cached speech changed while reading".into());
    }
    Ok(Some(bytes))
}
