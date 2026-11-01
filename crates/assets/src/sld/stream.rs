//! Sequential native cells retain only the previous/current decoded frame.
use super::*;
pub struct Stream {
    bytes: Vec<u8>,
    header: usize,
    pos: usize,
    count: usize,
    next: usize,
    current: Option<Frame>,
}
impl Stream {
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        let (count, header) = header(&bytes)?;
        if count == 0 && header != bytes.len() {
            return Err(invalid(FORMAT, header, "unparsed trailing bytes"));
        }
        Ok(Self {
            bytes,
            header,
            pos: header,
            count,
            next: 0,
            current: None,
        })
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Ascending source indices only; the immediately preceding cell can be reread.
    pub fn frame(&mut self, index: usize) -> Result<&Frame, Error> {
        if index >= self.count || index + 1 < self.next {
            return Err(invalid(FORMAT, self.pos, "nonsequential stream index"));
        }
        while self.next <= index {
            // MAX_SIDE and the three decoded planes additionally bound this
            // to 12 Mi pixels per frame, at most 96 MiB for old + new layers.
            let mut budget = MAX_TOTAL_PIXELS;
            let (frame, pos) = decode_frame(
                &self.bytes,
                self.pos,
                self.header,
                self.current.as_ref(),
                &mut budget,
            )?;
            if self.next + 1 == self.count && pos != self.bytes.len() {
                return Err(invalid(FORMAT, pos, "unparsed trailing bytes"));
            }
            self.current = Some(frame);
            self.pos = pos;
            self.next += 1;
        }
        self.current
            .as_ref()
            .ok_or_else(|| invalid(FORMAT, self.pos, "empty stream"))
    }
}
