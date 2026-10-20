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
) -> Result<[Option<std::ops::Range<usize>>; 23], JsValue> {
    // Slots match building_ui::sprite_slot. Slot 22 is the builder's five rows.
    let mut ranges = std::array::from_fn(|_| None);
    let stems = [
        "",
        "b_dark_house_age1",
        "b_west_lumber_camp_age2",
        "b_west_mining_camp_age2",
        "b_dark_mill_age1",
        "",
        "b_dark_barracks_age1",
        "b_dark_wall_palisade",
        "b_dark_outpost_age1",
    ];
    for slot in 0..23 {
        let (source, count, step) = match slot {
            0 | 4 => continue, // Multipart trial Town Center and Mill.
            1..=8 => {
                if slot == 5 {
                    ("de/g_fm1.dds".to_string(), 1, 1)
                } else {
                    (
                        format!("de/{}_x1.sld", stems[slot]),
                        if slot == 1 {
                            3
                        } else if slot == 7 {
                            5
                        } else {
                            1
                        },
                        1,
                    )
                }
            }
            9..=12 => (
                format!("de/b_misc_foundation_{}x{}_x1.sld", slot - 8, slot - 8),
                3,
                1,
            ),
            13..=21 => {
                let kind = slot - 13;
                let stem = if kind == 0 {
                    "b_dark_town_center_age1"
                } else if kind == 5 || kind == 7 {
                    "b_misc"
                } else {
                    stems[kind]
                };
                (
                    if stem == "b_misc" {
                        "de/b_misc_rubble_x1.sld".to_string()
                    } else {
                        format!("de/{stem}_rubble_x1.sld")
                    },
                    1,
                    1,
                )
            }
            _ => ("graphics.drs:[32, 112, 108, 115]:1496".to_string(), 75, 1),
        };
        let (source, count, step) = if !source_present(manifest, &source) {
            let trial = match slot {
                1 => Some((2223, 3, 1)),
                6 => Some((2683, 1, 1)),
                7 => Some((4534, 45, 9)),
                8 => Some((4330, 1, 1)),
                9..=12 => Some((236 + slot as u32 - 9, 3, 1)),
                _ => None,
            };
            trial.map_or((source.clone(), count, step), |(id, count, step)| {
                (
                    format!("graphics.drs:[32, 112, 108, 115]:{id}"),
                    count,
                    step,
                )
            })
        } else {
            (source, count, step)
        };
        let Some(frames) = source_frames(manifest, &source, count) else {
            if source_present(manifest, &source) {
                return Err(JsValue::from_str(&format!(
                    "Corrupt building art ({source})"
                )));
            }
            continue;
        };
        let start = selected.len();
        for index in frames.into_iter().step_by(step) {
            selected.push(index);
            domains.push(if slot == 5 {
                AtlasDomain::Terrain
            } else {
                AtlasDomain::Objects
            });
        }
        ranges[slot] = Some(start..selected.len());
    }
    // The dat table's Mill is its static body plus a separate 43-frame sail
    // sequence. Keeping those native parts avoids repeating the body texture.
    let body = "graphics.drs:[32, 112, 108, 115]:3483";
    let sails = "graphics.drs:[32, 112, 108, 115]:3482";
    if source_present(manifest, body) || source_present(manifest, sails) {
        let mut frames = source_frames(manifest, body, 1).ok_or("Invalid Mill body")?;
        frames.extend(source_frames(manifest, sails, 43).ok_or("Invalid Mill sails")?);
        let start = selected.len();
        for frame in frames {
            selected.push(frame);
            domains.push(AtlasDomain::Objects);
        }
        ranges[4] = Some(start..selected.len());
    }
    Ok(ranges)
}

/// A farm needs a local patch of the ploughed sheet, not its entire 1024² area.
/// Validate the original rectangle before selecting the 256² runtime window.
#[inline(never)]
pub(super) fn crop_farm(
    manifest: &mut Manifest,
    selected: &[usize],
    ranges: &[Option<std::ops::Range<usize>>; 23],
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
