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
    if extents.len() != MAX_REVIEWED_FRAMES as usize + 2540 || counts != [1474, 3435] {
        return Err("incomplete semantic selection".into());
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
    assert_eq!(prove(&manifest).unwrap(), (1474, 3435));
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
