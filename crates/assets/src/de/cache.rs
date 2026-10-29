//! Retain decoded frames for at most one source while writing a pack.
use crate::{Error, sld::Frame};

pub(super) struct Cache<F> {
    load: F,
    current: Option<(usize, Vec<Frame>)>,
}

impl<F: FnMut(usize) -> Result<Vec<Frame>, Error>> Cache<F> {
    pub fn new(load: F) -> Self {
        Self {
            load,
            current: None,
        }
    }

    pub fn get(&mut self, index: usize) -> Result<&[Frame], Error> {
        if self
            .current
            .as_ref()
            .is_none_or(|(current, _)| *current != index)
        {
            // Release the old decoded source before decoding its replacement.
            self.current = None;
            self.current = Some((index, (self.load)(index)?));
        }
        self.current
            .as_ref()
            .map(|(_, frames)| frames.as_slice())
            .ok_or_else(|| crate::invalid("DE", 0, "source cache is empty"))
    }
}

#[cfg(test)]
mod tests;
