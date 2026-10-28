use super::*;
use crate::building_ui::content::sprites;

pub(super) fn colour(
    ranges: &[Option<std::ops::Range<usize>>],
    semantic: usize,
    team: [u8; 3],
) -> [u8; 3] {
    use aoe_assets::catalog::units::Pose;
    if ranges
        .iter()
        .skip(Pose::NeutralSheepStand as usize)
        .take(4)
        .flatten()
        .any(|r| r.contains(&semantic))
    {
        [190, 190, 190]
    } else {
        team
    }
}

pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<Vec<Option<std::ops::Range<usize>>>, JsValue> {
    let mut ranges: Vec<Option<std::ops::Range<usize>>> = Vec::new();
    for (semantic, &source) in sprites().iter().enumerate() {
        let identity = source.manifest_source();
        let Some(frames) = source_frames(manifest, &identity, source.frames()) else {
            if source_present(manifest, &identity) {
                return Err(JsValue::from_str(
                    "Incomplete actor animation in the local pack",
                ));
            }
            ranges.push(None);
            continue;
        };
        if let Some(prior) = sprites()[..semantic]
            .iter()
            .enumerate()
            .find_map(|(i, old)| {
                (old.id == source.id
                    && old.stride == source.stride
                    && old.sample_step() == source.sample_step()
                    && aoe_assets::catalog::runtime::actor_maskable(i)
                        == aoe_assets::catalog::runtime::actor_maskable(semantic))
                .then(|| ranges[i].clone())
                .flatten()
            })
        {
            ranges.push(Some(prior));
            continue;
        }
        if source.selected_frames() > MAX_SELECTED_FRAMES.saturating_sub(selected.len()) {
            return Err(JsValue::from_str("Too many actor animation frames"));
        }
        let start = selected.len();
        for row in 0..5 {
            for phase in (0..usize::from(source.stride)).step_by(usize::from(source.sample_step()))
            {
                selected.push(frames[source.source_frame(row, phase)]);
                domains.push(AtlasDomain::Objects);
            }
        }
        ranges.push(Some(start..selected.len()));
    }
    Ok(ranges)
}

/// A loop whose source cells and hotspots never change is static in every
/// facing. Keep one frame per row so its packed phase cannot dirty the scene.
/// Resolve each pose separately: owned and neutral sheep use different colours.
pub(super) fn resolve(
    manifest: &Manifest,
    selected: &[usize],
    records: &[GameFrame],
    ranges: &[Option<std::ops::Range<usize>>],
) -> Vec<Vec<GameFrame>> {
    ranges
        .iter()
        .map(|range| {
            let Some(range) = range else {
                return Vec::new();
            };
            let stride = range.len() / 5;
            let cell = |index: usize| {
                let f = &manifest.frames[selected[index]];
                (
                    &f.source, f.page, f.x, f.y, f.width, f.height, f.anchor_x, f.anchor_y,
                )
            };
            if stride > 1
                && (0..5).all(|row| {
                    let start = range.start + row * stride;
                    (1..stride).all(|phase| cell(start) == cell(start + phase))
                })
            {
                (0..5)
                    .map(|row| records[range.start + row * stride])
                    .collect()
            } else {
                records[range.clone()].to_vec()
            }
        })
        .collect()
}

// Keep bounded startup group construction shared across array lengths.
#[inline(never)]
pub(super) fn fill_groups(
    target: &mut [Vec<GameFrame>],
    roles: &[AssetRole],
    ranges: &RoleRanges,
    records: &[GameFrame],
) {
    for (frames, role) in target.iter_mut().zip(roles) {
        *frames = group_frames(ranges, records, *role);
    }
}
