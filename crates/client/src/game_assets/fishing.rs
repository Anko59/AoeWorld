//! DAT-selected sprites, subsampled without changing the bounded atlas.
use super::*;

pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<[Option<std::ops::Range<usize>>; 12], JsValue> {
    // id, source frames, frames per facing, sample step, selected facing rows.
    let sources = [
        (3549, 34, 34, 2, 1),
        (1910, 49, 49, 7, 1),
        (1911, 49, 49, 7, 1),
        (1912, 49, 49, 7, 1),
        (1913, 49, 49, 7, 1),
        (420, 30, 30, 5, 1),
        (0, 0, 0, 1, 0),
        (444, 5, 1, 1, 5),
        (449, 5, 1, 1, 5),
        (446, 5, 1, 1, 5),
        (3575, 140, 28, 4, 5),
        (3980, 75, 15, 15, 5),
    ];
    let mut ranges = std::array::from_fn(|_| None);
    for (slot, (id, count, stride, step, rows)) in sources.into_iter().enumerate() {
        if rows == 0 {
            continue;
        }
        let source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
        let Some(frames) = source_frames(manifest, &source, count) else {
            if source_present(manifest, &source) {
                return Err(JsValue::from_str("Corrupt fishing art"));
            }
            continue;
        };
        let start = selected.len();
        for row in 0..rows {
            for frame in (0..stride).step_by(step) {
                selected.push(frames[row * stride + frame]);
                domains.push(AtlasDomain::Objects);
            }
        }
        ranges[slot] = Some(start..selected.len());
    }
    Ok(ranges)
}
