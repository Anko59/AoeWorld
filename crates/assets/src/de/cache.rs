//! Retain decoded frames for at most one source while writing a pack.
use crate::Error;

pub(super) struct Cache<T, F> {
    load: F,
    current: Option<(usize, Vec<T>)>,
}

impl<T, F: FnMut(usize) -> Result<Vec<T>, Error>> Cache<T, F> {
    pub fn new(load: F) -> Self {
        Self {
            load,
            current: None,
        }
    }

    pub fn get(&mut self, index: usize) -> Result<&mut [T], Error> {
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
            .as_mut()
            .map(|(_, frames)| frames.as_mut_slice())
            .ok_or_else(|| crate::invalid("DE", 0, "source cache is empty"))
    }
}

#[cfg(test)]
mod tests;
