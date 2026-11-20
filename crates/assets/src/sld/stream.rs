//! Sequential native cells retain the current frame and the last full layer of each kind.
use super::*;
pub struct Stream {
    bytes: Vec<u8>,
    header: usize,
    pos: usize,
    count: usize,
    next: usize,
    current: Option<Frame>,
    full_layers: FullLayers,
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
            full_layers: FullLayers::default(),
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
            // Up to three 48 MiB sets of layers can coexist during decoding:
            // the retained frame, the full reference layers and the new frame.
            let mut budget = MAX_TOTAL_PIXELS;
            let (frame, pos, full_layers) = decode_frame(
                &self.bytes,
                self.pos,
                self.header,
                &self.full_layers,
                &mut budget,
            )?;
            if self.next + 1 == self.count && pos != self.bytes.len() {
                return Err(invalid(FORMAT, pos, "unparsed trailing bytes"));
            }
            self.current = Some(frame);
            self.full_layers = full_layers;
            self.pos = pos;
            self.next += 1;
        }
        self.current
            .as_ref()
            .ok_or_else(|| invalid(FORMAT, self.pos, "empty stream"))
    }
}
