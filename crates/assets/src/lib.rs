//! Local-only, bounded classic asset conversion.
pub mod catalog;
pub mod drs;
pub mod pack;
pub mod palette;
pub mod slp;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid {format} at byte {offset}: {detail}")]
    Format {
        format: &'static str,
        offset: usize,
        detail: String,
    },
    #[error("unsupported {format} feature: {detail}")]
    Unsupported {
        format: &'static str,
        detail: String,
    },
    #[error("asset I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("PNG: {0}")]
    Png(#[from] png::EncodingError),
    #[error("PNG decode: {0}")]
    PngDecode(#[from] png::DecodingError),
}

fn invalid(format: &'static str, offset: usize, detail: impl Into<String>) -> Error {
    Error::Format {
        format,
        offset,
        detail: detail.into(),
    }
}

fn slice<'a>(
    bytes: &'a [u8],
    offset: usize,
    len: usize,
    format: &'static str,
) -> Result<&'a [u8], Error> {
    bytes
        .get(
            offset
                ..offset
                    .checked_add(len)
                    .ok_or_else(|| invalid(format, offset, "offset overflow"))?,
        )
        .ok_or_else(|| invalid(format, offset, "truncated data"))
}

fn u16_at(bytes: &[u8], offset: usize, format: &'static str) -> Result<u16, Error> {
    let part = slice(bytes, offset, 2, format)?;
    Ok(u16::from_le_bytes([part[0], part[1]]))
}

fn u32_at(bytes: &[u8], offset: usize, format: &'static str) -> Result<u32, Error> {
    let part = slice(bytes, offset, 4, format)?;
    Ok(u32::from_le_bytes([part[0], part[1], part[2], part[3]]))
}

fn i32_at(bytes: &[u8], offset: usize, format: &'static str) -> Result<i32, Error> {
    Ok(u32_at(bytes, offset, format)? as i32)
}
