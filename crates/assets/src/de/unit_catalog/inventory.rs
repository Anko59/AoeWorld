//! Local metadata for diagnosing the full native source budget before packing.
use super::*;

pub(super) fn write(install: &Path, selected: &[Selected]) -> Result<(), Error> {
    let buildings = super::super::buildings::resolve(install)?;
    let mut cells = BTreeMap::new();
    for source in buildings.sources.iter().cloned().chain(
        super::super::SELECTION
            .iter()
            .map(|s| format!("de/{}_x1.sld", s.stem)),
    ) {
        let bytes = super::super::read_bounded(
            &install
                .join(super::super::GRAPHICS)
                .join(source.trim_start_matches("de/")),
        )?;
        cells.insert(source, crate::u16_at(&bytes, 6, "DE inventory")?);
    }
    let units: BTreeMap<_, _> = selected.iter().map(|s| (s.sprite, s.cells)).collect();
    fs::write(
        "local-assets/de-import-inventory.json",
        serde_json::to_vec_pretty(&serde_json::json!({
            "unit_cells": units.values().map(|&n| u64::from(n)).sum::<u64>(),
            "building_and_misc_cells": cells.values().map(|&n| u64::from(n)).sum::<u64>(),
            "building_and_misc_sources": cells,
        }))?,
    )?;
    Ok(())
}
