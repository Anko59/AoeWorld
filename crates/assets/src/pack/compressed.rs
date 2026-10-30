//! Lossless, independently decodable colour/player/shadow frame payloads.
use crate::{Error, catalog::packing::MAX_FRAME_EXTENT, invalid};
use flate2::{Compression, Decompress, FlushDecompress, Status, write::ZlibEncoder};
use std::io::Write;

pub const MAX_ENCODED_FRAME_BYTES: usize = 64 * 1024 * 1024;

fn plane_bytes(size: [u16; 2]) -> Result<usize, Error> {
    if size
        .into_iter()
        .any(|side| side == 0 || side > MAX_FRAME_EXTENT)
    {
        return Err(invalid("frame", 0, "invalid frame dimensions"));
    }
    // The validated 2044-pixel side bound also bounds this multiplication.
    Ok(usize::from(size[0]) * usize::from(size[1]) * 4)
}

pub fn encode_frame(size: [u16; 2], planes: [&[u8]; 3]) -> Result<Vec<u8>, Error> {
    let length = plane_bytes(size)?;
    if planes.iter().any(|plane| plane.len() != length) {
        return Err(invalid("frame", 0, "invalid frame plane length"));
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    for side in size {
        encoder.write_all(&side.to_le_bytes())?;
    }
    for plane in planes {
        encoder.write_all(plane)?;
    }
    let encoded = encoder.finish()?;
    if encoded.len() > MAX_ENCODED_FRAME_BYTES {
        return Err(invalid("frame", 0, "encoded frame exceeds byte limit"));
    }
    Ok(encoded)
}

struct Decoder<'a> {
    bytes: &'a [u8],
    state: Decompress,
    ended: bool,
}

impl Decoder<'_> {
    fn fill(&mut self, output: &mut [u8]) -> Result<(), Error> {
        let mut filled = 0;
        while filled < output.len() {
            let before_in = self.state.total_in();
            let before_out = self.state.total_out();
            let input = self
                .bytes
                .get(before_in as usize..)
                .ok_or_else(|| invalid("frame", 0, "invalid compressed offset"))?;
            let status = self
                .state
                .decompress(input, &mut output[filled..], FlushDecompress::None)
                .map_err(|e| invalid("frame", 0, e.to_string()))?;
            filled += (self.state.total_out() - before_out) as usize;
            self.ended = status == Status::StreamEnd;
            if (self.ended && filled != output.len())
                || (self.state.total_in() == before_in && self.state.total_out() == before_out)
            {
                return Err(invalid("frame", 0, "incomplete compressed frame"));
            }
        }
        Ok(())
    }

    fn finish(mut self, expected: usize) -> Result<(), Error> {
        if !self.ended {
            let input = self
                .bytes
                .get(self.state.total_in() as usize..)
                .ok_or_else(|| invalid("frame", 0, "invalid compressed offset"))?;
            let status = self
                .state
                .decompress(input, &mut [0; 1], FlushDecompress::Finish)
                .map_err(|e| invalid("frame", 0, e.to_string()))?;
            self.ended = status == Status::StreamEnd;
        }
        if !self.ended
            || self.state.total_in() != self.bytes.len() as u64
            || self.state.total_out() != expected as u64
        {
            return Err(invalid("frame", 0, "compressed frame length mismatch"));
        }
        Ok(())
    }
}

pub fn decode_frame(bytes: &[u8], size: [u16; 2]) -> Result<[Vec<u8>; 3], Error> {
    let length = plane_bytes(size)?;
    if bytes.len() > MAX_ENCODED_FRAME_BYTES {
        return Err(invalid("frame", 0, "encoded frame exceeds byte limit"));
    }
    let mut decoder = Decoder {
        bytes,
        state: Decompress::new(true),
        ended: false,
    };
    let mut header = [0; 4];
    decoder.fill(&mut header)?;
    if [
        u16::from_le_bytes([header[0], header[1]]),
        u16::from_le_bytes([header[2], header[3]]),
    ] != size
    {
        return Err(invalid("frame", 0, "compressed frame dimensions disagree"));
    }
    let mut planes = std::array::from_fn(|_| vec![0; length]);
    for plane in &mut planes {
        decoder.fill(plane)?;
    }
    decoder.finish(4 + length * 3)?;
    Ok(planes)
}

#[cfg(test)]
#[path = "tests/compressed.rs"]
mod tests;
