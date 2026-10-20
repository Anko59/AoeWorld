use aoe_assets::{
    catalog::{
        packing::{AtlasDomain, FrameExtent, PAGE_SIDE, pack_frames},
        runtime,
    },
    pack::Manifest,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub fn check(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    let mut selected = BTreeSet::new();
    let mut extents = Vec::new();
    let mut add = |source: String,
                   count: u32,
                   step: usize,
                   domain|
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
        for frame in frames.into_iter().step_by(step) {
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
            if selected.insert((source.clone(), frame.frame)) {
                extents.push(FrameExtent {
                    domain,
                    width: frame.width,
                    height: frame.height,
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
                1,
                if source.terrain_topology.is_some() {
                    AtlasDomain::Terrain
                } else {
                    AtlasDomain::Objects
                },
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
                if building.unit_id == 72 {
                    usize::from(part.frames)
                } else {
                    1
                },
                AtlasDomain::Objects,
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
        "packed {} frames in the fixed three-page atlas",
        placements.len()
    );
    Ok(())
}
