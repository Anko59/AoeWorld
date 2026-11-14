//! Exact, view-bounded packet retention shared by both GPU backends.
use super::Sprite;
use std::ops::Range;

const STRIDE: usize = std::mem::size_of::<Sprite>();

/// Callers enforce their existing 64 MiB packet bound before visiting this cache.
/// Keep bytes, rather than float equality or a collision-prone content hash.
#[derive(Default)]
pub(crate) struct InstanceUpload {
    previous: Vec<u8>,
}

impl InstanceUpload {
    pub(crate) fn changed(&self, bytes: &[u8]) -> Range<usize> {
        debug_assert!(bytes.len().is_multiple_of(STRIDE));
        let start = bytes
            .chunks_exact(STRIDE)
            .zip(self.previous.chunks_exact(STRIDE))
            .position(|(new, old)| new != old)
            .map_or(bytes.len().min(self.previous.len()), |i| i * STRIDE);
        if start == bytes.len() {
            return 0..0;
        }
        let end = bytes
            .chunks_exact(STRIDE)
            .enumerate()
            .rposition(|(i, packet)| {
                self.previous.get(i * STRIDE..(i + 1) * STRIDE) != Some(packet)
            })
            .map_or(start, |i| (i + 1) * STRIDE);
        start..end
    }

    /// Commit only after an accepted GPU submission. A refused frame changes
    /// neither the GPU packet nor this snapshot. Shrinks retain an equal prefix.
    pub(crate) fn commit(&mut self, bytes: &[u8], changed: Range<usize>) {
        if bytes.len() != self.previous.len() {
            self.previous.clear();
            self.previous.reserve_exact(bytes.len());
            self.previous.extend_from_slice(bytes);
        } else {
            self.previous[changed.clone()].copy_from_slice(&bytes[changed]);
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.previous.clear();
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        self.previous.capacity()
    }
}
