//! Bounded building subset from trial and optional DE overlay packs.
use super::*;

/// Optional overlay is served from a private pack link made by assets-import-de.
#[inline(never)]
pub(super) async fn overlay(manifest: &mut Manifest) -> Result<(), JsValue> {
    let response: Response = JsFuture::from(
        web_sys::window()
            .ok_or("No window")?
            .fetch_with_str("/asset-pack/buildings/manifest.json"),
    )
    .await?
    .dyn_into()?;
    if response.status() == 404 {
        return Ok(());
    }
    if !response.ok() {
        return Err(JsValue::from_str("Building pack unavailable"));
    }
    let bytes = Uint8Array::new(&JsFuture::from(response.array_buffer()?).await?).to_vec();
    let mut extra = decode_manifest(&bytes)?;
    if extra.version != 1
        || extra.pages.len() > 256
        || extra.frames.len() > aoe_assets::pack::MAX_FRAMES
        || manifest.pages.len() + extra.pages.len() > u16::MAX as usize
    {
        return Err(JsValue::from_str("Invalid building pack"));
    }
    let offset = manifest.pages.len() as u16;
    let chunk_offset = u16::try_from(manifest.chunks.len()).map_err(error)?;
    if manifest.chunks.len() + extra.chunks.len() > aoe_assets::pack::frame_chunks::MAX_CHUNKS {
        return Err(error("Too many native chunks"));
    }
    for chunk in &mut extra.chunks {
        if chunk.file.contains('/')
            || chunk.file.contains('\\')
            || chunk.file == ".."
            || chunk.file == "."
        {
            return Err(error("Invalid native chunk name"));
        }
        chunk.file = format!("buildings/{}", chunk.file);
    }
    for page in &mut extra.pages {
        for name in [&mut page.color, &mut page.player, &mut page.shadow] {
            if name.contains('/') || name.contains('\\') {
                return Err(JsValue::from_str("Invalid building page"));
            }
            *name = format!("buildings/{name}");
        }
    }
    for frame in &mut extra.frames {
        if let Some(location) = &mut frame.native {
            if usize::from(location.chunk) >= extra.chunks.len() {
                return Err(error("Invalid native chunk index"));
            }
            location.chunk += chunk_offset;
            continue;
        }
        if usize::from(frame.page) >= extra.pages.len() {
            return Err(JsValue::from_str("Invalid building page index"));
        }
        frame.page += offset;
    }
    if extra.building_data != "00" {
        let mut recipes =
            super::recipes::decode(&extra.building_data, &extra.frames).map_err(error)?;
        for b in &mut recipes {
            for p in b
                .parts
                .iter_mut()
                .chain(&mut b.construction)
                .chain(b.states.iter_mut().flat_map(|(_, p)| p))
                .chain(b.damage.iter_mut().flat_map(|d| &mut d.parts))
            {
                if let Some(record) = &mut p.record {
                    *record += manifest.frames.len();
                }
            }
        }
        manifest
            .buildings
            .retain(|old| !recipes.iter().any(|new| new.unit_id == old.unit_id));
        manifest.buildings.extend(recipes);
    }
    if !extra.terrains.is_empty() {
        manifest.terrains = extra.terrains;
    }
    manifest.pages.extend(extra.pages);
    manifest.chunks.extend(extra.chunks);
    manifest.frames.extend(extra.frames);
    Ok(())
}

/// The licensed DE install supplies the waypoint flag absent from the trial.
/// Sample its 90-cell loop into ten cells; classic MFLAG remains the base pack.
pub(super) fn rally(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<Option<std::ops::Range<usize>>, JsValue> {
    let source = "de/b_misc_waypoint_flag_x1.sld";
    let Some(frames) = source_frames(manifest, source, 90) else {
        if source_present(manifest, source) {
            return Err(error("Corrupt rally flag art"));
        }
        return Ok(None);
    };
    if selected.len() > MAX_SELECTED_FRAMES - 10 {
        return Err(error("Too many selected flag frames"));
    }
    let start = selected.len();
    selected.extend(frames.into_iter().step_by(9));
    domains.extend(std::iter::repeat_n(AtlasDomain::Objects, 10));
    Ok(Some(start..selected.len()))
}

pub(super) fn rally_frames(
    records: &[GameFrame],
    range: Option<std::ops::Range<usize>>,
    classic: Vec<GameFrame>,
) -> Vec<GameFrame> {
    range.map_or(classic, |r| records[r].to_vec())
}
