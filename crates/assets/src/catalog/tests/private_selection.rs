//! Match the runtime's supplemental building and ambient selections.
use super::*;

pub(super) fn append(
    manifest: &Manifest,
    extents: &mut Vec<FrameExtent>,
    counts: &mut [usize; 2],
) -> Result<(), String> {
    let present = |source: &str| manifest.frames.iter().any(|f| f.source == source);
    let native = |id| {
        manifest
            .buildings
            .iter()
            .any(|b| b.unit_id == id && b.parts.iter().any(|p| p.available))
    };
    let stages = |side| {
        let buildings: Vec<_> = manifest
            .buildings
            .iter()
            .filter(|b| b.footprint == [side; 2])
            .collect();
        !buildings.is_empty()
            && buildings
                .iter()
                .all(|b| b.construction.iter().any(|p| p.available))
    };
    let mut plan = crate::catalog::buildings::supplemental(present, native, stages, |id| {
        manifest
            .buildings
            .iter()
            .any(|b| b.unit_id == id && b.construction.iter().any(|p| p.available))
    });
    plan.extend(
        crate::catalog::runtime::FISHING
            .into_iter()
            .map(
                |(_, id, frames, step)| crate::catalog::buildings::Selection {
                    slot: 0,
                    source: format!("graphics.drs:[32, 112, 108, 115]:{id}"),
                    frames,
                    step,
                    take: usize::MAX,
                    resident: true,
                    required: false,
                },
            ),
    );
    for item in plan {
        if !present(&item.source) && !item.required {
            continue;
        }
        let mut frames: Vec<_> = manifest
            .frames
            .iter()
            .filter(|f| f.source == item.source && f.frame < item.frames)
            .collect();
        frames.sort_by_key(|f| f.frame);
        if frames.len() != item.frames as usize
            || frames.iter().enumerate().any(|(i, f)| f.frame != i as u32)
        {
            return Err(format!("incomplete supplemental source: {}", item.source));
        }
        if !item.resident {
            continue;
        }
        for frame in frames.into_iter().step_by(item.step).take(item.take) {
            let page = manifest
                .pages
                .get(usize::from(frame.page))
                .ok_or("missing supplemental page")?;
            if page.width != PAGE_SIDE
                || page.height != PAGE_SIDE
                || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
                || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
            {
                return Err("invalid supplemental rectangle".into());
            }
            let terrain = item.slot == 5;
            extents.push(FrameExtent {
                domain: crate::catalog::buildings::domain(item.slot),
                width: frame.width,
                height: frame.height,
            });
            counts[usize::from(!terrain)] += 1;
        }
    }
    Ok(())
}

pub(super) fn overlay(
    path: &std::path::Path,
    manifest: &mut Manifest,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path
        .parent()
        .ok_or("missing pack directory")?
        .join("buildings/manifest.json");
    if !path.exists() {
        return Ok(());
    }
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("overlay manifest exceeds 32MiB".into());
    }
    let mut extra: Manifest = serde_json::from_slice(&bytes)?;
    let offset = u16::try_from(manifest.pages.len())?;
    for frame in &mut extra.frames {
        frame.page = frame
            .page
            .checked_add(offset)
            .ok_or("overlay page overflow")?;
    }
    manifest.pages.extend(extra.pages);
    manifest.frames.extend(extra.frames);
    Ok(())
}

pub(super) fn duplicate_actor(index: usize) -> bool {
    let source = super::super::units::SPRITES[index];
    super::super::units::SPRITES[..index]
        .iter()
        .enumerate()
        .any(|(i, old)| {
            old.id == source.id
                && old.stride == source.stride
                && old.sample_step() == source.sample_step()
                && super::super::runtime::actor_maskable(i)
                    == super::super::runtime::actor_maskable(index)
        })
}

pub(super) fn actor_frames() -> usize {
    super::super::units::SPRITES
        .iter()
        .enumerate()
        .filter(|(index, source)| !duplicate_actor(*index) && source.base_role().is_none())
        .map(|(_, s)| s.selected_frames())
        .sum()
}

#[test]
fn unsupported_general_source_topology_is_rejected() {
    let mut manifest = super::synthetic_manifest();
    let mut extra = manifest
        .frames
        .iter()
        .find(|f| f.source == "de/u_cav_hero_joan_idleA_x1.sld")
        .expect("hero source")
        .clone();
    extra.frame = 5;
    manifest.frames.push(extra);
    assert!(
        super::prove(&manifest).is_err(),
        "unsupported hero source must be regenerated"
    );
    manifest.frames.pop();
}
