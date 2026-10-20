use aoe_assets::{
    catalog::{
        packing::{ATLAS_BYTES, AtlasDomain, FrameExtent, PAGE_COUNT, PAGE_SIDE, pack_frames},
        runtime,
    },
    pack::Manifest,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub fn check(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    let overlay = path.join("buildings/manifest.json");
    if overlay.is_file() {
        let mut extra: Manifest = serde_json::from_slice(&fs::read(overlay)?)?;
        if extra.version != 1
            || extra.pages.len() > 256
            || extra.frames.len() > 25000
            || manifest.pages.len() + extra.pages.len() > u16::MAX as usize
        {
            return Err("invalid overlay manifest".into());
        }
        let offset = manifest.pages.len() as u16;
        for frame in &mut extra.frames {
            if usize::from(frame.page) >= extra.pages.len() {
                return Err("invalid overlay frame page".into());
            }
            frame.page += offset;
        }
        manifest.pages.extend(extra.pages);
        manifest.frames.extend(extra.frames);
    }
    let mut selected = BTreeSet::new();
    let mut extents = Vec::new();
    let mut add = |source: String,
                   count: u32,
                   step: usize,
                   domain,
                   actor_copy: bool,
                   take: usize,
                   resident: bool|
     -> Result<(), Box<dyn std::error::Error>> {
        let mut frames = manifest
            .frames
            .iter()
            .filter(|f| f.source == source && f.frame < count)
            .collect::<Vec<_>>();
        frames.sort_by_key(|f| f.frame);
        if count == 0
            || step == 0
            || frames.len() != count as usize
            || frames
                .iter()
                .enumerate()
                .any(|(number, f)| f.frame != number as u32)
        {
            return Err(format!("missing or duplicate {source} frame prefix").into());
        }
        for frame in frames.into_iter().step_by(step).take(take) {
            let page = manifest
                .pages
                .get(usize::from(frame.page))
                .ok_or("missing source page")?;
            if page.width != PAGE_SIDE
                || page.height != PAGE_SIDE
                || u32::from(frame.x) + u32::from(frame.width) > u32::from(page.width)
                || u32::from(frame.y) + u32::from(frame.height) > u32::from(page.height)
            {
                return Err(format!("invalid {source} source rectangle").into());
            }
            if resident && (selected.insert((source.clone(), frame.frame)) || actor_copy) {
                extents.push(FrameExtent {
                    domain,
                    width: (if source == "de/g_fm1.dds" {
                        frame.width.min(256)
                    } else {
                        frame.width
                    })
                    .div_ceil(aoe_assets::catalog::buildings::pixel_step(&source)),
                    height: (if source == "de/g_fm1.dds" {
                        frame.height.min(256)
                    } else {
                        frame.height
                    })
                    .div_ceil(aoe_assets::catalog::buildings::pixel_step(&source)),
                });
            }
        }
        Ok(())
    };
    for (sources, required) in [
        (runtime::REQUIRED.as_slice(), true),
        (runtime::OPTIONAL_RESOURCES.as_slice(), false),
        (runtime::OPTIONAL_TERRAIN.as_slice(), false),
    ] {
        for source in sources {
            let identity = source.manifest_source();
            if !required && !manifest.frames.iter().any(|f| f.source == identity) {
                continue;
            }
            add(
                identity,
                source.frames,
                match source.role {
                    aoe_assets::catalog::AssetRole::CavalryStanding => 10,
                    aoe_assets::catalog::AssetRole::VillagerStanding => 15,
                    _ => 1,
                },
                if source.terrain_topology.is_some() {
                    AtlasDomain::Terrain
                } else {
                    AtlasDomain::Objects
                },
                false,
                usize::MAX,
                true,
            )?;
        }
    }
    // The loader packs every actor group separately: neutral sheep need their
    // own grey player ramp even when their source is shared with owned sheep.
    for source in aoe_assets::catalog::units::SPRITES {
        let identity = source.manifest_source();
        if manifest.frames.iter().any(|f| f.source == identity) {
            add(
                identity,
                source.frames(),
                usize::from(source.sample_step()),
                AtlasDomain::Objects,
                true,
                usize::MAX,
                true,
            )?;
        }
    }
    for building in manifest
        .buildings
        .iter()
        .filter(|b| aoe_assets::catalog::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id))
    {
        if let Some(slp) = building.foundation_slp
            && !runtime::REQUIRED
                .iter()
                .chain(&runtime::OPTIONAL_TERRAIN)
                .any(|s| s.id as i32 == slp)
        {
            add(
                format!("terrain.drs:[32, 112, 108, 115]:{slp}"),
                u32::from(building.foundation_size[0]) * u32::from(building.foundation_size[1]),
                1,
                AtlasDomain::Terrain,
                false,
                usize::MAX,
                true,
            )?;
        }
        for part in building
            .parts
            .iter()
            .chain(&building.construction)
            .filter(|p| p.available)
        {
            add(
                part.source.clone(),
                u32::from(part.frames) * u32::from(part.angles),
                runtime::building_frame_step(
                    building.unit_id,
                    part.frames,
                    part.angles,
                    part.frame_rate,
                ),
                AtlasDomain::Objects,
                false,
                usize::MAX,
                true,
            )?;
        }
    }
    let native = |id| {
        manifest
            .buildings
            .iter()
            .any(|b| b.unit_id == id && b.parts.iter().any(|p| p.available))
    };
    let stages = |side| {
        let matching = manifest
            .buildings
            .iter()
            .filter(|b| {
                aoe_assets::catalog::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id)
                    && b.footprint == [side; 2]
            })
            .collect::<Vec<_>>();
        !matching.is_empty()
            && matching
                .iter()
                .all(|b| b.construction.iter().any(|p| p.available))
    };
    for item in aoe_assets::catalog::buildings::supplemental(
        |source| manifest.frames.iter().any(|f| f.source == source),
        native,
        stages,
        |id| {
            manifest
                .buildings
                .iter()
                .any(|b| b.unit_id == id && b.construction.iter().any(|p| p.available))
        },
    ) {
        if !manifest.frames.iter().any(|f| f.source == item.source) && !item.required {
            continue;
        }
        add(
            item.source,
            item.frames,
            item.step,
            if item.slot == 5 {
                AtlasDomain::Terrain
            } else {
                AtlasDomain::Objects
            },
            true,
            item.take,
            item.resident,
        )?;
    }
    for (_, id, frames, step) in runtime::FISHING {
        let source = format!("graphics.drs:[32, 112, 108, 115]:{id}");
        if manifest.frames.iter().any(|f| f.source == source) {
            add(
                source,
                frames,
                step,
                AtlasDomain::Objects,
                true,
                usize::MAX,
                true,
            )?;
        }
    }
    println!("selected {} frames", extents.len());
    println!(
        "object pixels {}",
        extents
            .iter()
            .filter(|f| f.domain == AtlasDomain::Objects)
            .map(|f| u64::from(f.width) * u64::from(f.height))
            .sum::<u64>()
    );
    for building in &manifest.buildings {
        println!(
            "unit {} complete {:?} construction {:?}",
            building.unit_id,
            building
                .parts
                .iter()
                .map(|p| (p.slp_id, p.frames, p.frame_rate, p.available))
                .collect::<Vec<_>>(),
            building
                .construction
                .iter()
                .map(|p| (p.slp_id, p.frames, p.frame_rate, p.available))
                .collect::<Vec<_>>()
        );
    }
    for (source, number) in &selected {
        if *number == 0 {
            let group = manifest
                .frames
                .iter()
                .filter(|f| f.source == *source && selected.contains(&(source.clone(), f.frame)))
                .collect::<Vec<_>>();
            if group
                .iter()
                .map(|f| u32::from(f.width) * u32::from(f.height))
                .sum::<u32>()
                > 100000
            {
                println!(
                    "{source}: {} frames, {} pixels, first {}x{}",
                    group.len(),
                    group
                        .iter()
                        .map(|f| u32::from(f.width) * u32::from(f.height))
                        .sum::<u32>(),
                    group[0].width,
                    group[0].height
                );
            }
        }
    }
    let placements = pack_frames(&extents)?;
    println!(
        "packed {} frames in {PAGE_COUNT} pages ({ATLAS_BYTES} bytes)",
        placements.len()
    );
    Ok(())
}
