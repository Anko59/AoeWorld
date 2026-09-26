use super::{ElevationGrid, HydrologyKind, WaterModelProvenance};

pub(super) fn model_river_surfaces(
    axis: u16,
    kinds: &[HydrologyKind],
    elevation: &ElevationGrid,
    levels: &mut [Option<i32>],
    provenance: &mut [WaterModelProvenance],
) {
    for index in 0..kinds.len() {
        if kinds[index] != HydrologyKind::River || levels[index].is_some() {
            continue;
        }
        let x = (index % usize::from(axis)) as u16;
        let y = (index / usize::from(axis)) as u16;
        levels[index] = Some(elevation.target_height(axis, x, y));
        provenance[index] = WaterModelProvenance::ModelledRiverSurface;
    }
}
