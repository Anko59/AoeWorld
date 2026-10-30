//! Bounded chunk storage for independently compressed native animation cells.
use super::{MAX_FRAMES, compressed};
use crate::{Error, invalid};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const TARGET_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_CHUNKS: usize = 2048;
pub const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameChunk {
    pub file: String,
    pub hash: String,
    pub bytes: u32,
}
impl FrameChunk {
    pub fn from_bytes(file: String, bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > compressed::MAX_ENCODED_FRAME_BYTES {
            return Err(invalid("frame chunk", 0, "invalid chunk payload length"));
        }
        Ok(Self {
            file,
            hash: blake3::hash(bytes).to_hex().to_string(),
            bytes: bytes.len() as u32,
        })
    }
    pub fn verify_payload(&self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() != self.bytes as usize
            || bytes.is_empty()
            || bytes.len() > compressed::MAX_ENCODED_FRAME_BYTES
            || blake3::hash(bytes).to_hex().as_str() != self.hash
        {
            return Err(invalid(
                "frame chunk",
                0,
                "native chunk length or hash mismatch",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FrameLocation {
    pub chunk: u16,
    pub offset: u32,
    pub length: u32,
}
impl FrameLocation {
    pub fn slice<'a>(&self, bytes: &'a [u8]) -> Result<&'a [u8], Error> {
        if self.length == 0 || self.length as usize > compressed::MAX_ENCODED_FRAME_BYTES {
            return Err(invalid("frame chunk", 0, "invalid frame payload length"));
        }
        let start = self.offset as usize;
        let end = start
            .checked_add(self.length as usize)
            .ok_or_else(|| invalid("frame chunk", 0, "frame offset overflow"))?;
        bytes
            .get(start..end)
            .ok_or_else(|| invalid("frame chunk", 0, "frame outside chunk"))
    }
}
pub struct Writer {
    directory: PathBuf,
    buffer: Vec<u8>,
    chunks: Vec<FrameChunk>,
    seen: BTreeMap<[u8; 32], FrameLocation>,
    frames: usize,
    bytes: usize,
}
impl Writer {
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_owned(),
            buffer: Vec::new(),
            chunks: Vec::new(),
            seen: BTreeMap::new(),
            frames: 0,
            bytes: 0,
        }
    }
    /// The caller aborts its temporary pack if a storage operation fails.
    pub fn push(&mut self, size: [u16; 2], planes: [&[u8]; 3]) -> Result<FrameLocation, Error> {
        if self.frames == MAX_FRAMES {
            return Err(invalid("frame chunk", 0, "too many native frames"));
        }
        let encoded = compressed::encode_frame(size, planes)?;
        let digest = *blake3::hash(&encoded).as_bytes();
        if let Some(location) = self.seen.get(&digest) {
            self.frames += 1;
            return Ok(*location);
        }
        let total = self
            .bytes
            .checked_add(encoded.len())
            .filter(|n| *n <= MAX_TOTAL_BYTES)
            .ok_or_else(|| invalid("frame chunk", 0, "native frame bytes exceed limit"))?;
        if !self.buffer.is_empty() && self.buffer.len() + encoded.len() > TARGET_CHUNK_BYTES {
            self.flush()?;
        }
        if self.chunks.len() == MAX_CHUNKS {
            return Err(invalid("frame chunk", 0, "too many frame chunks"));
        }
        let location = FrameLocation {
            chunk: self.chunks.len() as u16,
            offset: self.buffer.len() as u32,
            length: encoded.len() as u32,
        };
        self.buffer.extend_from_slice(&encoded);
        self.bytes = total;
        self.frames += 1;
        self.seen.insert(digest, location);
        if self.buffer.len() >= TARGET_CHUNK_BYTES {
            self.flush()?;
        }
        Ok(location)
    }
    fn flush(&mut self) -> Result<(), Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        if self.chunks.len() >= MAX_CHUNKS {
            return Err(invalid("frame chunk", 0, "too many frame chunks"));
        }
        let name = format!("frames-{:04}.bin", self.chunks.len());
        fs::create_dir_all(&self.directory)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join(&name))?;
        file.write_all(&self.buffer)?;
        self.chunks
            .push(FrameChunk::from_bytes(name, &self.buffer)?);
        self.buffer.clear();
        Ok(())
    }
    pub fn finish(mut self) -> Result<Vec<FrameChunk>, Error> {
        self.flush()?;
        Ok(self.chunks)
    }
}
#[cfg(test)]
#[path = "tests/frame_chunks.rs"]
mod tests;
