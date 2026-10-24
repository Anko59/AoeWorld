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
        || extra.frames.len() > 25000
        || manifest.pages.len() + extra.pages.len() > u16::MAX as usize
    {
        return Err(JsValue::from_str("Invalid building pack"));
    }
    let offset = manifest.pages.len() as u16;
    for page in &mut extra.pages {
        for name in [&mut page.color, &mut page.player, &mut page.shadow] {
            if name.contains('/') || name.contains('\\') {
                return Err(JsValue::from_str("Invalid building page"));
            }
            *name = format!("buildings/{name}");
        }
    }
    for frame in &mut extra.frames {
        if usize::from(frame.page) >= extra.pages.len() {
            return Err(JsValue::from_str("Invalid building page index"));
        }
        frame.page += offset;
    }
    manifest.pages.extend(extra.pages);
    manifest.frames.extend(extra.frames);
    Ok(())
}

#[inline(never)]
pub(super) fn select(
    manifest: &Manifest,
    selected: &mut Vec<usize>,
    domains: &mut Vec<AtlasDomain>,
) -> Result<[Option<std::ops::Range<usize>>; 45], JsValue> {
    let mut ranges: [Option<std::ops::Range<usize>>; 45] = std::array::from_fn(|_| None);
    let plan = aoe_assets::catalog::buildings::supplemental(
        |source| source_present(manifest, source),
        |id| native(manifest, id),
        |side| native_stages(manifest, side),
        |id| {
            manifest
                .buildings
                .iter()
                .any(|b| b.unit_id == id && b.construction.iter().any(|p| p.record.is_some()))
        },
    );
    for item in plan {
        let Some(frames) = source_frames(manifest, &item.source, item.frames) else {
            if item.required || source_present(manifest, &item.source) {
                return Err(error(format!("Corrupt building art ({})", item.source)));
            }
            continue;
        };
        if !item.resident {
            continue;
        }
        let start = selected.len();
        for index in frames.into_iter().step_by(item.step).take(item.take) {
            selected.push(index);
            domains.push(if item.slot == 5 {
                AtlasDomain::Terrain
            } else {
                AtlasDomain::Objects
            });
        }
        if let Some(range) = &mut ranges[item.slot] {
            range.end = selected.len();
        } else {
            ranges[item.slot] = Some(start..selected.len());
        }
    }
    Ok(ranges)
}

/// A farm needs a local patch of the ploughed sheet, not its entire 1024² area.
/// Validate the original rectangle before selecting the 256² runtime window.
#[inline(never)]
pub(super) fn crop_farm(
    manifest: &mut Manifest,
    selected: &[usize],
    ranges: &[Option<std::ops::Range<usize>>; 45],
) -> Result<(), JsValue> {
    if let Some(range) = &ranges[5] {
        for semantic in range.clone() {
            let frame = &mut manifest.frames[selected[semantic]];
            let page = manifest
                .pages
                .get(usize::from(frame.page))
                .ok_or("Invalid farm page")?;
            if u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
                || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
            {
                return Err(JsValue::from_str("Invalid farm source bounds"));
            }
            frame.width = frame.width.min(256);
            frame.height = frame.height.min(256);
        }
    }
    Ok(())
}

pub(super) fn native(manifest: &Manifest, id: i32) -> bool {
    manifest
        .buildings
        .iter()
        .any(|b| b.unit_id == id && b.parts.iter().any(|p| p.record.is_some()))
}

fn native_stages(manifest: &Manifest, side: f32) -> bool {
    let mut found = false;
    for building in &manifest.buildings {
        if building.footprint == [side; 2] {
            found = true;
            if !building
                .construction
                .iter()
                .any(|part| part.record.is_some())
            {
                return false;
            }
        }
    }
    found
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
