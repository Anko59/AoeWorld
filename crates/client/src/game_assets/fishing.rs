//! DAT-selected sprites, subsampled without changing the bounded atlas.
use super::*;

pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<[Option<std::ops::Range<usize>>; 12], JsValue> {
    let mut ranges = std::array::from_fn(|_| None);
    for (slot, id, count, step) in aoe_assets::catalog::runtime::FISHING {
        let source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
        let Some(frames) = source_frames(manifest, &source, count) else {
            if source_present(manifest, &source) {
                return Err(JsValue::from_str("Corrupt fishing art"));
            }
            continue;
        };
        let start = selected.len();
        for index in frames.into_iter().step_by(step) {
            selected.push(index);
            domains.push(AtlasDomain::Objects);
        }
        ranges[slot] = Some(start..selected.len());
    }
    Ok(ranges)
}
