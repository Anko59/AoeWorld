//! Keep native semantic geometry while deduplicating resident pixel identities.
use super::*;

pub(in crate::game_assets) fn prepare(
    manifest: &Manifest,
    selected: &[usize],
    maskable: &[bool],
    ranges: &[Option<std::ops::Range<usize>>],
    team: [u8; 3],
    placements: &[Placement],
    records: &mut [GameFrame],
) -> Result<Option<NativeFrames>, JsValue> {
    let mut cells = Vec::new();
    let mut identities = BTreeMap::new();
    for (semantic, &index) in selected.iter().enumerate() {
        let f = &manifest.frames[index];
        let Some(location) = f.native else {
            continue;
        };
        let cell = Cell {
            location,
            size: [f.width, f.height],
            owned: maskable[semantic],
            team: actors::colour(ranges, semantic, team),
        };
        let id = if let Some(&id) = identities.get(&cell) {
            id
        } else {
            let id = cells.len() as u32 + 1;
            cells.push(cell);
            identities.insert(cell, id);
            id
        };
        records[semantic].stream = id;
    }
    if cells.is_empty() {
        return Ok(None);
    }
    NativeFrames::new(
        manifest.chunks.clone(),
        cells,
        placement::pages(placements) as u16,
    )
    .map(Some)
}
