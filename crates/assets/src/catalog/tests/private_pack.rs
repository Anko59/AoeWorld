//! Explicit native metadata packing proof. Original manifests are never fixtures.
use super::{
    MAX_REVIEWED_FRAMES, OPTIONAL_RESOURCE_SOURCES, OPTIONAL_TERRAIN_SOURCES,
    REQUIRED_RENDER_SOURCES,
    packing::{
        ATLAS_BYTES, AtlasDomain, FrameExtent, MAX_SELECTED_FRAMES, PAGE_COUNT, PAGE_SIDE,
        pack_frames,
    },
};
use crate::pack::Manifest;
use std::io::Read;

const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;

fn reviewed_sources() -> impl Iterator<Item = (String, u32, AtlasDomain)> {
    REQUIRED_RENDER_SOURCES
        .iter()
        .chain(&OPTIONAL_RESOURCE_SOURCES)
        .chain(&OPTIONAL_TERRAIN_SOURCES)
        .map(|s| {
            (
                s.manifest_source(),
                s.frames,
                if s.terrain_topology.is_some() {
                    AtlasDomain::Terrain
                } else {
                    AtlasDomain::Objects
                },
            )
        })
        .chain(
            super::units::SPRITES
                .iter()
                .map(|s| (s.manifest_source(), s.frames(), AtlasDomain::Objects)),
        )
}

fn prove(manifest: &Manifest) -> Result<(usize, usize), String> {
    if manifest.version != 1 || manifest.frames.len() > 25_000 || manifest.pages.len() > 256 {
        return Err("unsupported or oversized manifest".into());
    }
    let mut extents = Vec::new();
    let mut selected = std::collections::BTreeSet::new();
    let mut counts = [0_usize; 2];
    // Conservative full-prefix proof in production semantic order. The client
    // omits unused idle cells and adds the separately checked DE building subset.
    for (identity, count, domain) in reviewed_sources() {
        let mut frames = manifest
            .frames
            .iter()
            .filter(|frame| frame.source == identity && frame.frame < count)
            .collect::<Vec<_>>();
        frames.sort_unstable_by_key(|frame| frame.frame);
        if frames.len() != count as usize
            || frames
                .iter()
                .enumerate()
                .any(|(index, frame)| frame.frame != index as u32)
        {
            return Err(format!("missing or duplicate reviewed prefix: {identity}"));
        }
        counts[usize::from(domain == AtlasDomain::Objects)] += frames.len();
        for frame in frames {
            selected.insert((frame.source.clone(), frame.frame));
            let page = manifest
                .pages
                .get(usize::from(frame.page))
                .ok_or("missing source page")?;
            if page.width != PAGE_SIDE
                || page.height != PAGE_SIDE
                || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
                || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
            {
                return Err("invalid source rectangle".into());
            }
            extents.push(FrameExtent {
                domain,
                width: frame.width,
                height: frame.height,
            });
        }
    }
    if extents.len() != MAX_REVIEWED_FRAMES as usize + 2695 || counts != [1474, 3590] {
        return Err("incomplete semantic selection".into());
    }
    // Match the loader's DAT additions after the static catalog, including
    // every native animation frame. Shared sources are packed only once.
    for (unit_id, part) in manifest
        .buildings
        .iter()
        .filter(|b| super::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id))
        .flat_map(|b| {
            b.parts
                .iter()
                .chain(&b.construction)
                .map(move |p| (b.unit_id, p))
        })
        .filter(|(_, p)| p.available)
    {
        let wanted = u32::from(part.frames) * u32::from(part.angles);
        if part.frames == 0 || part.angles == 0 || wanted > 256 {
            return Err("invalid DAT building frame layout".into());
        }
        let mut frames = manifest
            .frames
            .iter()
            .filter(|f| f.source == part.source && f.frame < wanted)
            .collect::<Vec<_>>();
        frames.sort_by_key(|f| f.frame);
        if frames.len() != wanted as usize
            || frames.iter().enumerate().any(|(i, f)| f.frame != i as u32)
        {
            return Err("incomplete DAT building source".into());
        }
        // The loader retains one damage frame per Palisade angle. Every other
        // recipe retains its complete frame/angle prefix, including static art.
        let step = if unit_id == 72 {
            usize::from(part.frames)
        } else {
            1
        };
        for frame in frames.into_iter().step_by(step) {
            if selected.insert((frame.source.clone(), frame.frame)) {
                let page = manifest
                    .pages
                    .get(usize::from(frame.page))
                    .ok_or("missing building page")?;
                if page.width != PAGE_SIDE
                    || page.height != PAGE_SIDE
                    || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
                    || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
                {
                    return Err("invalid building source rectangle".into());
                }
                extents.push(FrameExtent {
                    domain: AtlasDomain::Objects,
                    width: frame.width,
                    height: frame.height,
                });
                counts[1] += 1;
            }
        }
    }
    for building in manifest
        .buildings
        .iter()
        .filter(|b| super::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id))
    {
        let Some(slp) = building.foundation_slp else {
            continue;
        };
        if REQUIRED_RENDER_SOURCES
            .iter()
            .chain(&OPTIONAL_TERRAIN_SOURCES)
            .any(|s| s.id as i32 == slp)
        {
            continue;
        }
        let source = format!("terrain.drs:[32, 112, 108, 115]:{slp}");
        let [columns, rows] = building.foundation_size;
        let count = u32::from(columns) * u32::from(rows);
        for number in 0..count {
            let frame = manifest
                .frames
                .iter()
                .find(|f| f.source == source && f.frame == number)
                .ok_or("missing DAT foundation frame")?;
            if selected.insert((frame.source.clone(), frame.frame)) {
                extents.push(FrameExtent {
                    domain: AtlasDomain::Terrain,
                    width: frame.width,
                    height: frame.height,
                });
                counts[0] += 1;
            }
        }
    }
    let placements = pack_frames(&extents).map_err(|error| error.to_string())?;
    if placements.len() != extents.len() {
        return Err("truncated placements".into());
    }
    for (index, (frame, placed)) in extents.iter().zip(&placements).enumerate() {
        let correct_domain = match frame.domain {
            AtlasDomain::Terrain => placed.page < 2,
            AtlasDomain::Objects => (2..4).contains(&placed.page),
        };
        if !correct_domain
            || placed.width != frame.width
            || placed.height != frame.height
            || placed.x < 2
            || placed.y < 2
            || u32::from(placed.x) + u32::from(placed.width) >= u32::from(PAGE_SIDE)
            || u32::from(placed.y) + u32::from(placed.height) >= u32::from(PAGE_SIDE)
        {
            return Err("invalid runtime placement".into());
        }
        for other in &placements[..index] {
            if placed.page == other.page
                && u32::from(placed.x) < u32::from(other.x) + u32::from(other.width)
                && u32::from(other.x) < u32::from(placed.x) + u32::from(placed.width)
                && u32::from(placed.y) < u32::from(other.y) + u32::from(other.height)
                && u32::from(other.y) < u32::from(placed.y) + u32::from(placed.height)
            {
                return Err("overlapping runtime rectangles".into());
            }
        }
    }
    if MAX_SELECTED_FRAMES != 5120 || PAGE_COUNT != 4 || ATLAS_BYTES != 67_108_864 {
        return Err("physical budget changed".into());
    }
    Ok((counts[0], counts[1]))
}

#[test]
#[ignore = "explicit private manifest required; run with --ignored and AOE_REVIEWED_PACK_MANIFEST"]
fn original_manifest_all_reviewed_frames_fit_fixed_atlas() -> Result<(), Box<dyn std::error::Error>>
{
    let path = std::env::var_os("AOE_REVIEWED_PACK_MANIFEST")
        .ok_or("AOE_REVIEWED_PACK_MANIFEST is required")?;
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("manifest exceeds 32MiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    let (terrain, objects) = prove(&manifest)?;
    eprintln!(
        "private metadata proof input={} selected={} terrain={} objects={} pages={} bytes={}",
        manifest.input_hash,
        terrain + objects,
        terrain,
        objects,
        PAGE_COUNT,
        ATLAS_BYTES
    );
    Ok(())
}

#[test]
fn portable_proof_rejects_missing_duplicate_and_corrupt_source_rectangles() {
    let mut manifest = synthetic_manifest();
    assert_eq!(prove(&manifest).unwrap(), (1474, 3590));
    let saved = manifest.frames[0].clone();
    manifest.frames[0].frame = 1;
    assert!(prove(&manifest).is_err());
    manifest.frames[0] = saved.clone();
    manifest.frames[0].width = 0;
    assert!(prove(&manifest).is_err());
    manifest.frames[0] = saved.clone();
    manifest.frames[0].width = 2045;
    assert!(prove(&manifest).is_err());
    manifest.frames[0] = saved.clone();
    manifest.frames[0].page = u16::MAX;
    assert!(prove(&manifest).is_err());
    manifest.frames[0] = saved;
    manifest.frames[0].x = PAGE_SIDE;
    assert!(prove(&manifest).is_err());
    manifest.frames.pop();
    assert!(prove(&manifest).is_err());
}

fn synthetic_manifest() -> Manifest {
    let page = crate::pack::AtlasPage {
        color: "fixture.png".into(),
        color_hash: String::new(),
        player: "fixture.png".into(),
        player_hash: String::new(),
        shadow: "fixture.png".into(),
        shadow_hash: String::new(),
        outline: "fixture.png".into(),
        outline_hash: String::new(),
        width: PAGE_SIDE,
        height: PAGE_SIDE,
    };
    let mut manifest = Manifest {
        version: 1,
        converter: "synthetic".into(),
        input_hash: "synthetic-not-original-proof".into(),
        pages: vec![page],
        frames: Vec::new(),
        buildings: Vec::new(),
        building_data: "00".into(),
    };
    for (identity, count, _) in reviewed_sources() {
        // Neutral sheep reuse each owned source with another runtime palette.
        // A source manifest contains those frames once.
        if manifest.frames.iter().any(|f| f.source == identity) {
            continue;
        }
        for frame in 0..count {
            manifest.frames.push(crate::pack::FrameRecord {
                source: identity.clone(),
                source_hash: String::new(),
                frame,
                page: 0,
                x: 0,
                y: 0,
                width: 2,
                height: 2,
                anchor_x: -7,
                anchor_y: 175,
            });
        }
    }
    manifest
}

#[test]
fn private_proof_covers_house_variants_and_all_construction_angles() {
    let mut manifest = synthetic_manifest();
    let part = |slp, frames, angles| crate::dat::BuildingPart {
        graphic_id: slp,
        slp_id: slp,
        source: format!("fixture:{slp}"),
        available: true,
        frames,
        angles,
        primary: true,
        frame_rate: 0.0,
        offset: [0; 2],
        misplacement: [0.0; 2],
        layer: 20,
    };
    manifest.buildings.push(crate::dat::BuildingComposition {
        unit_id: 70,
        footprint: [2.0; 2],
        foundation_terrain: 2,
        foundation_slp: None,
        foundation_size: [0; 2],
        parts: vec![part(2223, 3, 1)],
        construction: (236..=239).map(|slp| part(slp, 1, 3)).collect(),
    });
    for slp in [2223, 236, 237, 238, 239] {
        for frame in 0..3 {
            let mut record = manifest.frames[0].clone();
            record.source = format!("fixture:{slp}");
            record.frame = frame;
            manifest.frames.push(record);
        }
    }
    assert_eq!(prove(&manifest).unwrap(), (1474, 3590 + 15));
    for index in manifest.frames.len() - 15..manifest.frames.len() {
        let mut bad = manifest.clone();
        bad.frames.remove(index);
        assert!(prove(&bad).is_err(), "missing record {index}");
        let mut bad = manifest.clone();
        bad.frames.push(bad.frames[index].clone());
        assert!(prove(&bad).is_err(), "duplicate record {index}");
        let mut bad = manifest.clone();
        bad.frames[index].x = PAGE_SIDE;
        assert!(prove(&bad).is_err(), "invalid rectangle {index}");
    }
}
