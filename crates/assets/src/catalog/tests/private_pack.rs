//! Explicit native metadata packing proof. Original manifests are never fixtures.
use super::{
    MAX_REVIEWED_FRAMES, OPTIONAL_RESOURCE_SOURCES, OPTIONAL_TERRAIN_SOURCES,
    REQUIRED_RENDER_SOURCES,
    packing::{AtlasDomain, FrameExtent, MAX_LOGICAL_FRAMES, PAGE_SIDE, pack_resident_frames},
};
use crate::pack::Manifest;
use selection::actor_frames;
use std::io::Read;

const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;

#[path = "private_castle.rs"]
mod castle;
#[path = "private_occupancy.rs"]
mod occupancy;
#[path = "private_selection.rs"]
mod selection;

fn reviewed_sources() -> impl Iterator<
    Item = (
        String,
        u32,
        AtlasDomain,
        usize,
        Option<super::units::ActorSource>,
    ),
> {
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
                } else if super::runtime::PLAYER_ROLES.contains(&s.role) {
                    AtlasDomain::Objects
                } else {
                    AtlasDomain::Shared
                },
                match s.role {
                    super::AssetRole::CavalryStanding => 10,
                    super::AssetRole::VillagerStanding => 15,
                    _ => 1,
                },
                None,
            )
        })
        .chain(
            super::units::SPRITES
                .iter()
                .enumerate()
                .filter(|(index, _)| !selection::duplicate_actor(*index))
                .map(|(index, s)| {
                    (
                        s.manifest_source(),
                        s.frames(),
                        if super::runtime::actor_maskable(index) {
                            AtlasDomain::Objects
                        } else {
                            AtlasDomain::Shared
                        },
                        usize::from(s.sample_step()),
                        Some(*s),
                    )
                }),
        )
}

fn prove(manifest: &Manifest) -> Result<(usize, usize), String> {
    if manifest.version != 1
        || manifest.frames.len() > 2 * crate::pack::MAX_FRAMES
        || manifest.pages.len() > 512
    {
        return Err("unsupported or oversized manifest".into());
    }
    let mut extents = Vec::new();
    let mut selected = std::collections::BTreeSet::new();
    let mut counts = [0_usize; 2];
    // Validate complete prefixes, then pack the client's actual selection:
    // compact idle facings and the sampled native actor/projectile cells.
    // Neutral/owned palette copies still occupy separate semantic entries.
    for (identity, count, domain, step, actor) in reviewed_sources() {
        if actor.is_some_and(|s| s.is_native())
            && manifest
                .frames
                .iter()
                .filter(|f| f.source == identity)
                .count()
                != count as usize
        {
            return Err("unsupported native hero topology".into());
        }
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
        // Each semantic actor keeps its own complete native direction blocks.
        let drawn = actor.map(|source| {
            (0..source.rows())
                .flat_map(|row| {
                    (0..usize::from(source.stride))
                        .step_by(usize::from(source.sample_step()))
                        .map(move |phase| source.source_frame(row, phase) as u32)
                })
                .collect::<Vec<_>>()
        });
        for (number, frame) in frames.into_iter().enumerate() {
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
            // The one-cell stone is copied into five semantic facing rows,
            // exactly as the loader does; all five occupy the unchanged budget.
            let copies = drawn
                .as_ref()
                .map_or(usize::from(number % step == 0), |cells| {
                    cells.iter().filter(|&&cell| cell == frame.frame).count()
                });
            for _ in 0..copies {
                selected.insert((frame.source.clone(), frame.frame));
                counts[usize::from(domain != AtlasDomain::Terrain)] += 1;
                extents.push(FrameExtent {
                    domain,
                    width: frame.width,
                    height: frame.height,
                });
            }
        }
    }
    let actor_count = actor_frames();
    // 45 cavalry idle and 70 villager idle cells are not drawn.
    if extents.len() != MAX_REVIEWED_FRAMES as usize + actor_count - 115
        || counts != [1474, 897 + actor_count - 115]
    {
        return Err("incomplete semantic selection".into());
    }
    // Match the loader's DAT additions after the static catalog, including
    // the native wake sampling. Shared sources are packed only once.
    for (unit_id, part) in manifest
        .buildings
        .iter()
        .filter(|b| super::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id))
        .flat_map(|b| {
            b.parts
                .iter()
                .chain(&b.construction)
                .map(move |p| (b.unit_id, p))
                .chain(
                    b.damage
                        .iter()
                        .flat_map(|d| d.parts.iter().map(|p| (-1, p))),
                )
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
        // recipe uses the shared runtime selection, including wake sampling.
        let step =
            super::runtime::building_frame_step(unit_id, part.frames, part.angles, part.frame_rate);
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
    selection::append(manifest, &mut extents, &mut counts)?;
    let placements = pack_resident_frames(&extents).map_err(|error| error.to_string())?;
    if placements.len() != extents.len() {
        return Err("truncated placements".into());
    }
    let mut occupied = occupancy::Occupied::default();
    for (frame, placed) in extents.iter().zip(&placements) {
        let correct_domain = match frame.domain {
            AtlasDomain::Terrain => placed.page < 2,
            AtlasDomain::Objects => {
                (2..5).contains(&placed.page) || (placed.page >= 8 && placed.page % 2 == 0)
            }
            AtlasDomain::Shared => placed.page < 5 || (placed.page >= 8 && placed.page % 2 == 0),
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
        occupied.insert(*placed)?;
    }
    if extents.len() > MAX_LOGICAL_FRAMES || placements.iter().any(|p| p.page >= 64) {
        return Err("physical budget changed".into());
    }
    Ok((counts[0], counts[1]))
}

#[test]
#[ignore = "explicit private manifest required; run with --ignored and AOE_REVIEWED_PACK_MANIFEST"]
fn original_manifest_all_reviewed_frames_fit_bounded_atlas()
-> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("AOE_REVIEWED_PACK_MANIFEST")
        .ok_or("AOE_REVIEWED_PACK_MANIFEST is required")?;
    let path = std::path::PathBuf::from(path);
    let file = std::fs::File::open(&path)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("manifest exceeds 32MiB".into());
    }
    let mut manifest: Manifest = serde_json::from_slice(&bytes)?;
    selection::overlay(&path, &mut manifest)?;
    let (terrain, objects) = prove(&manifest)?;
    eprintln!(
        "private metadata proof input={} selected={} terrain={} objects={} max_layers={} max_bytes={}",
        manifest.input_hash,
        terrain + objects,
        terrain,
        objects,
        64,
        64 * 2048 * 2048 * 4_u64
    );
    Ok(())
}

#[test]
fn portable_proof_rejects_missing_duplicate_and_corrupt_source_rectangles() {
    let mut manifest = synthetic_manifest();
    assert_eq!(
        actor_frames(),
        92473,
        // Complete current DAT unit cells, including neutral colour semantics.
        // Pin the complete current selection independently of runtime residency.
        "military, Castle, projectile and farmer cells included"
    );
    assert_eq!(
        prove(&manifest).unwrap(),
        (1474, 897 + actor_frames() - 115)
    );
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
        chunks: Vec::new(),
        version: 1,
        converter: "synthetic".into(),
        input_hash: "synthetic-not-original-proof".into(),
        pages: vec![page],
        frames: Vec::new(),
        buildings: Vec::new(),
        building_data: "00".into(),
    };
    for (identity, count, _, _, _) in reviewed_sources() {
        // Neutral sheep reuse each owned source with another runtime palette.
        // A source manifest contains those frames once.
        if manifest.frames.iter().any(|f| f.source == identity) {
            continue;
        }
        for frame in 0..count {
            manifest.frames.push(crate::pack::FrameRecord {
                native: None,
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
fn full_catalog_with_native_mill_dock_and_farm_fits_runtime_budget() {
    let mut manifest = synthetic_manifest();
    // Independent metadata fixture: disjoint native Mill body/sails, Dock wake
    // and Farm terrain. Do not manufacture original pixels for this proof.
    for (unit_id, count, terrain) in [(68, 44, false), (45, 60, false), (50, 36, true)] {
        let source = format!("fixture:{unit_id}");
        for frame in 0..count {
            let mut record = manifest.frames[0].clone();
            record.source = source.clone();
            record.frame = frame;
            manifest.frames.push(record);
        }
        manifest.buildings.push(crate::dat::BuildingComposition {
            unit_id,
            footprint: [3.0; 2],
            foundation_terrain: 0,
            foundation_slp: terrain.then_some(unit_id),
            foundation_size: if terrain { [6, 6] } else { [0, 0] },
            parts: if terrain {
                Vec::new()
            } else {
                vec![crate::dat::BuildingPart {
                    graphic_id: unit_id,
                    slp_id: unit_id,
                    source,
                    available: true,
                    frames: count as u16,
                    angles: 1,
                    primary: true,
                    frame_rate: 0.15,
                    offset: [0; 2],
                    misplacement: [0.0; 2],
                    layer: 20,
                }]
            },
            construction: Vec::new(),
            damage: Vec::new(),
        });
        if terrain {
            for frame in manifest.frames.iter_mut().rev().take(count as usize) {
                frame.source = format!("terrain.drs:[32, 112, 108, 115]:{unit_id}");
            }
        }
    }
    assert_eq!(
        prove(&manifest).unwrap(),
        (1510, 897 + actor_frames() - 115 + 74)
    );
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
        damage: Vec::new(),
    });
    for slp in [2223, 236, 237, 238, 239] {
        for frame in 0..3 {
            let mut record = manifest.frames[0].clone();
            record.source = format!("fixture:{slp}");
            record.frame = frame;
            manifest.frames.push(record);
        }
    }
    assert_eq!(
        prove(&manifest).unwrap(),
        (1474, 897 + actor_frames() - 115 + 15)
    );
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
