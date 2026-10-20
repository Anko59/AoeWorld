use aoe_assets::{
    catalog::{
        packing::{AtlasDomain, FrameExtent, pack_frames},
        runtime,
    },
    pack::Manifest,
};
use std::{collections::BTreeSet, fs, path::PathBuf};

pub fn check(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let manifest: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    let mut selected = BTreeSet::new();
    let mut extents = Vec::new();
    let mut add = |source: String, count: u32, domain| -> Result<(), Box<dyn std::error::Error>> {
        for number in 0..count {
            let frame = manifest
                .frames
                .iter()
                .find(|f| f.source == source && f.frame == number)
                .ok_or_else(|| format!("missing {source} frame {number}"))?;
            if selected.insert((source.clone(), number)) {
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
                if part.frame_rate > 0.0 {
                    u32::from(part.frames)
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
