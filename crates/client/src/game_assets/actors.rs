use super::*;
use aoe_assets::catalog::units::SPRITES;

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
    let mut ranges = Vec::new();
    for source in SPRITES {
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
        if frames.len() > MAX_SELECTED_FRAMES.saturating_sub(selected.len()) {
            return Err(JsValue::from_str("Too many actor animation frames"));
        }
        let start = selected.len();
        for row in 0..5 {
            for phase in (0..usize::from(source.stride)).step_by(usize::from(source.sample_step()))
            {
                selected.push(frames[row * usize::from(source.stride) + phase]);
                domains.push(AtlasDomain::Objects);
            }
        }
        ranges.push(Some(start..selected.len()));
    }
    Ok(ranges)
}
