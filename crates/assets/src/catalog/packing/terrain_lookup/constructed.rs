//! Immutable ownership proves that a freshly constructed row remains validated.
use super::{GROUP_COUNT, LookupError, Metadata, TerrainGroup, ValidatedTable, write_new_table};

/// An owned atlas admitted only through the strict zero-row constructor.
///
/// Private fields and immutable access keep arbitrary raw uploads distinct from
/// validated layouts. This proves layout bounds, not artwork approval or identity.
/// No second atlas, descriptor vector, or decoded row is retained.
pub struct ConstructedTerrainAtlas {
    pixels: Vec<u8>,
    metadata: Metadata,
}

impl ConstructedTerrainAtlas {
    pub fn new(
        mut pixels: Vec<u8>,
        groups: &[TerrainGroup<'_>; GROUP_COUNT],
    ) -> Result<Self, LookupError> {
        let metadata = write_new_table(&mut pixels, groups)?;
        Ok(Self { pixels, metadata })
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The constructor validated every record and owns immutable row bytes.
    /// Reading this view requires no decoder, checksum scan or overlap rescan.
    pub fn table(&self) -> ValidatedTable<'_> {
        super::view::constructed_view(&self.pixels, self.metadata)
    }

    /// Consuming ownership deliberately discards the constructed-layout proof.
    /// Subsequent raw uploads must not reactivate world sampling from these bytes.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}
