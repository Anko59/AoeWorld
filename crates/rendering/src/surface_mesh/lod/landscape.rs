use super::*;

// Geometry and canonical appearance share the same candidate storage and
// distance/tile tie comparison, but only geometry rejects invisible fine tiles.
pub(super) fn retain_best(slot: &mut Option<CellSample>, candidate: CellSample) {
    if slot.is_none_or(|best| {
        candidate.distance < best.distance
            || (candidate.distance == best.distance && candidate.source_tile < best.source_tile)
    }) {
        *slot = Some(candidate);
    }
}

pub(super) fn assign(cells: &mut [Option<CellSample>], owners: &[Option<CellSample>]) {
    for (cell, owner) in cells.iter_mut().zip(owners) {
        if let (Some(cell), Some(owner)) = (cell, owner) {
            cell.sample.appearance = owner.sample.appearance;
        }
    }
}
