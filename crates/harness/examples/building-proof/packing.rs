use aoe_assets::{
    catalog::{
        packing::{ATLAS_BYTES, AtlasDomain, FrameExtent, PAGE_COUNT, PAGE_SIDE, pack_frames},
        runtime,
    },
    pack::Manifest,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

#[path = "reclaim.rs"]
mod reclaim;
use reclaim::Item;

pub fn check(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    let mut dirs = manifest
        .pages
        .iter()
        .map(|_| path.clone())
        .collect::<Vec<_>>();
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
        dirs.extend(extra.pages.iter().map(|_| path.join("buildings")));
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
    // Per-category (frames, runtime texels, native source pixels) for the report.
    let mut items = Vec::<Item>::new();
    let mut usage = std::collections::BTreeMap::<&'static str, [u64; 3]>::new();
    let mut add = |kind: &'static str,
                   source: String,
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
                    width: if source == "de/g_fm1.dds" {
                        frame.width.min(256)
                    } else {
                        frame.width
                    },
                    height: if source == "de/g_fm1.dds" {
                        frame.height.min(256)
                    } else {
                        frame.height
                    },
                });
                let mut counted = frame.clone();
                if source == "de/g_fm1.dds" {
                    counted.width = counted.width.min(256);
                    counted.height = counted.height.min(256);
                }
                items.push(Item {
                    kind,
                    frame: counted,
                });
                let last = extents.last().ok_or("extent")?;
                let row = usage.entry(kind).or_default();
                row[0] += 1;
                row[1] += u64::from(last.width) * u64::from(last.height);
                row[2] += u64::from(frame.width) * u64::from(frame.height);
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
                if source.terrain_topology.is_some() {
                    "terrain"
                } else {
                    "objects: resources and other"
                },
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
                "units",
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
                "terrain",
                format!("terrain.drs:[32, 112, 108, 115]:{slp}"),
                u32::from(building.foundation_size[0]) * u32::from(building.foundation_size[1]),
                1,
                AtlasDomain::Terrain,
                false,
                usize::MAX,
                true,
            )?;
        }
        let complete = building.parts.len();
        for (number, part) in building
            .parts
            .iter()
            .chain(&building.construction)
            .enumerate()
            .filter(|(_, p)| p.available)
        {
            add(
                if number < complete {
                    "buildings: DAT bodies"
                } else {
                    "construction: DAT stages"
                },
                part.source.clone(),
                u32::from(part.frames) * u32::from(part.angles),
                1,
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
            match item.slot {
                26..=31 | 38..=41 | 44 => "buildings: Feudal supplemental",
                32..=37 | 13..=21 | 42..=43 => "rubble and gate",
                9..=12 | 24 => "construction: foundations",
                _ => "buildings: Dark supplemental and ambient",
            },
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
                "fishing",
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
    println!("category | frames | runtime texels | native source pixels");
    for (kind, [frames, texels, native]) in &usage {
        println!("  {kind} | {frames} | {texels} | {native}");
    }
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
    let mut by_source = std::collections::BTreeMap::<&str, (u64, u64)>::new();
    for item in &items {
        let row = by_source.entry(&item.frame.source).or_default();
        row.0 += 1;
        row.1 += u64::from(item.frame.width) * u64::from(item.frame.height);
    }
    let mut largest = by_source.into_iter().collect::<Vec<_>>();
    largest.sort_by_key(|(_, (_, pixels))| std::cmp::Reverse(*pixels));
    for (source, (frames, pixels)) in largest.iter().take(40) {
        println!("source {source}: {frames} frames, {pixels} px");
    }
    reclaim::report(&items, &manifest.pages, &dirs)?;
    let placements = pack_frames(&extents)?;
    let mut page_pixels = [[0_u64; 2]; PAGE_COUNT];
    for (placed, extent) in placements.iter().zip(&extents) {
        let row = &mut page_pixels[usize::from(placed.page)];
        row[usize::from(extent.domain == AtlasDomain::Objects)] +=
            u64::from(placed.width) * u64::from(placed.height);
    }
    let page_area = u64::from(PAGE_SIDE) * u64::from(PAGE_SIDE);
    for (page, [terrain, objects]) in page_pixels.iter().enumerate() {
        println!(
            "page {page}: terrain {terrain} px, objects {objects} px, {:.1}% of {page_area}",
            (terrain + objects) as f64 * 100.0 / page_area as f64
        );
    }
    println!(
        "packed {} frames in {PAGE_COUNT} pages ({ATLAS_BYTES} bytes)",
        placements.len()
    );
    Ok(())
}
