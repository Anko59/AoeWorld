use super::Terrain;
use aoe_map::{Depletion, ResourceOverlayError};

impl Terrain {
    /// Applies a deterministic resource depletion to map terrain. Exhausted
    /// resources immediately stop blocking passability through the overlay.
    pub fn deplete_resource(
        &mut self,
        id: u64,
        requested: u16,
    ) -> Result<Depletion, ResourceOverlayError> {
        let Self::Map { generator, overlay } = self else {
            return Err(ResourceOverlayError::UnknownResource);
        };
        overlay.deplete(generator, id, requested)
    }

    pub(crate) fn update_mutable_state_hash(&self, hash: &mut blake3::Hasher) {
        let Self::Map { overlay, .. } = self else {
            hash.update(&[0]);
            return;
        };
        hash.update(&[1]);
        hash.update(&overlay.revision().to_le_bytes());
        hash.update(&(overlay.changed_count() as u64).to_le_bytes());
        for (id, remaining) in overlay.changed() {
            hash.update(&id.to_le_bytes());
            hash.update(&remaining.to_le_bytes());
        }
    }
}
