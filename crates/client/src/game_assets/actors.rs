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
    let base_end = selected.len();
    for (semantic, &source) in sprites().iter().enumerate() {
        let identity = source.manifest_source();
        if (source.rows() == 16 || source.id == 4500)
            && source_present(manifest, &identity)
            && manifest
                .frames
                .iter()
                .filter(|f| f.source == identity)
                .count()
                != source.frames() as usize
        {
            return Err(JsValue::from_str("Unsupported native actor topology"));
        }
        let Some(frames) = source_frames(manifest, &identity, source.frames()) else {
            if source_present(manifest, &identity) {
                return Err(JsValue::from_str(
                    "Incomplete actor animation in the local pack",
                ));
            }
            ranges.push(None);
            continue;
        };
        if let Some(role) = source.base_role() {
            let resident = if role == AssetRole::CavalryStanding {
                (0..5)
                    .map(|row| frames[source.source_frame(row, 0)])
                    .collect()
            } else {
                frames.clone()
            };
            if let Some(start) = selected[..base_end]
                .windows(resident.len())
                .enumerate()
                .find_map(|(start, window)| {
                    (window == resident
                        && domains[start..start + resident.len()]
                            .iter()
                            .all(|d| *d == AtlasDomain::Objects))
                    .then_some(start)
                })
            {
                ranges.push(Some(start..start + resident.len()));
                continue;
            }
        }
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
        for row in 0..source.rows() {
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
        .enumerate()
        .map(|(semantic, range)| {
            let Some(range) = range else {
                return Vec::new();
            };
            let source = sprites()[semantic];
            let indices: Vec<_> = if source.base_role() == Some(AssetRole::CavalryWalking)
                && range.len() == source.frames() as usize
            {
                (0..5)
                    .flat_map(|row| {
                        (0..usize::from(source.stride))
                            .step_by(usize::from(source.sample_step()))
                            .map(move |phase| range.start + source.source_frame(row, phase))
                    })
                    .collect()
            } else {
                range.clone().collect()
            };
            let rows = source.rows();
            let stride = indices.len() / rows;
            let cell = |index: usize| {
                let f = &manifest.frames[selected[indices[index]]];
                (
                    &f.source, f.page, f.x, f.y, f.width, f.height, f.anchor_x, f.anchor_y,
                )
            };
            if stride > 1
                && (0..rows).all(|row| {
                    let start = row * stride;
                    (1..stride).all(|phase| cell(start) == cell(start + phase))
                })
            {
                (0..rows)
                    .map(|row| records[indices[row * stride]])
                    .collect()
            } else {
                indices.into_iter().map(|index| records[index]).collect()
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
