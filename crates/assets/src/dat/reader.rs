//! Bounds-checked little-endian cursor for the genie data file.
use crate::{Error, invalid, slice};

pub(super) const FORMAT: &str = "empires2.dat";

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    pub(super) fn fail(&self, detail: impl Into<String>) -> Error {
        invalid(FORMAT, self.pos, detail)
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let part = slice(self.bytes, self.pos, len, FORMAT)?;
        self.pos += len;
        Ok(part)
    }

    pub(super) fn skip(&mut self, len: usize) -> Result<(), Error> {
        self.take(len).map(|_| ())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(super) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(super) fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub(super) fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub(super) fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// A count read from the file, rejected above `max` before any allocation.
    pub(super) fn bounded(&mut self, value: usize, max: usize, what: &str) -> Result<usize, Error> {
        if value > max {
            return Err(self.fail(format!("{what} {value} exceeds {max}")));
        }
        Ok(value)
    }
}

/// A fixed-width, NUL-padded text field.
pub(super) fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
