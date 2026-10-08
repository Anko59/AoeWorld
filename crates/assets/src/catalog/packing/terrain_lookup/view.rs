//! Immutable, once-validated CPU view; descriptor reads do not rescan the row.
use super::*;

/// Borrowing the atlas prevents safe code from replacing or mutating the row
/// while this view is alive. Cached metadata alone does not provide that proof.
pub struct ValidatedTable<'a> {
    row: &'a [u8],
    metadata: Metadata,
}

impl ValidatedTable<'_> {
    pub fn metadata(&self) -> Metadata {
        self.metadata
    }

    /// Constant-time read after complete validation at construction.
    pub fn descriptor(&self, index: u16) -> Result<Placement, LookupError> {
        if index >= self.metadata.total {
            return Err(LookupError::Index);
        }
        Ok(rectangle(self.row, usize::from(index)))
    }

    /// Group selection preserves signed periodic phase and accent hash wrapping.
    pub fn frame(&self, group: usize, x: i32, y: i32) -> Result<Placement, LookupError> {
        let index = self
            .metadata
            .frame_index(group, x, y)
            .ok_or(LookupError::Index)?;
        self.descriptor(index)
    }
}

/// Only construction validates the complete row, including overlaps/checksum.
/// Legacy zero rows yield None; an invalid replacement cannot create a view.
/// No pixels/metadata are copied or allocated by this borrowed view.
pub fn validated_table(atlas: &[u8]) -> Result<Option<ValidatedTable<'_>>, LookupError> {
    let Some(metadata) = decode(atlas)? else {
        return Ok(None);
    };
    Ok(Some(ValidatedTable {
        row: &atlas[ROW_OFFSET..ROW_OFFSET + ROW_BYTES],
        metadata,
    }))
}
