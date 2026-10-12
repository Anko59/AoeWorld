use super::*;

#[test]
fn catalog_has_unique_roles_and_nonempty_frame_ranges() {
    let mut roles = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(OPTIONAL_RESOURCE_SOURCES.iter())
        .chain(OPTIONAL_TERRAIN_SOURCES.iter())
        .map(|source| source.role)
        .collect::<Vec<_>>();
    roles.sort_unstable();
    roles.dedup();
    assert_eq!(
        roles.len(),
        REQUIRED_RENDER_SOURCES.len()
            + OPTIONAL_RESOURCE_SOURCES.len()
            + OPTIONAL_TERRAIN_SOURCES.len()
    );
    assert!(
        REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .all(|source| source.frames > 0)
    );
    assert_eq!(VERSION, 9);
    assert_eq!(AssetRole::StoneDeposit as usize, 13);
    assert_eq!(AssetRole::TreeConifer as usize, 14);
    assert_eq!(AssetRole::TreePalm as usize, 15);
    assert_eq!(AssetRole::Gravel as usize, 22);
    assert_eq!(AssetRole::Snow as usize, 24);
    assert_eq!(OPTIONAL_TERRAIN_SOURCES[8].id, 15024);
    assert_eq!(
        OPTIONAL_TERRAIN_SOURCES[4..8]
            .iter()
            .map(|source| (source.role, source.id, source.frames))
            .collect::<Vec<_>>(),
        vec![
            (AssetRole::GrassDirt, 15006, 100),
            (AssetRole::WornGrass, 15009, 100),
            (AssetRole::LushGrass, 15001, 100),
            (AssetRole::Gravel, 15017, 100),
        ]
    );
    assert!(OPTIONAL_TERRAIN_SOURCES[4..8].iter().all(|source| {
        source
            .terrain_topology
            .is_some_and(|topology| topology.supports_frames(source.frames))
    }));
    assert_eq!(OPTIONAL_TERRAIN_SOURCES[0].id, 15011);
    assert_eq!(OPTIONAL_TERRAIN_SOURCES[0].frames, 10);
    assert_eq!(AssetRole::WaterMedium as usize, 16);
    assert_eq!(AssetRole::Shallows as usize, 18);
    // Water depth sheets: light shallow water stays the required sheet;
    // medium, deep and walkable marsh shallows are optional periodic sheets.
    assert_eq!(REQUIRED_RENDER_SOURCES[6].id, 15002);
    assert_eq!(
        OPTIONAL_TERRAIN_SOURCES[1..4]
            .iter()
            .map(|source| (
                source.role,
                source.id,
                source.frames,
                source.terrain_topology
            ))
            .collect::<Vec<_>>(),
        [
            (AssetRole::WaterMedium, 15016),
            (AssetRole::WaterDeep, 15015),
            (AssetRole::Shallows, 15014),
        ]
        .map(|(role, id)| (
            role,
            id,
            100,
            Some(TerrainFrameTopology::PeriodicXMajorReversedY {
                columns: 10,
                rows: 10
            })
        ))
    );
    assert_eq!(
        REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .map(|source| source.frames as usize)
            .sum::<usize>(),
        MAX_REVIEWED_FRAMES as usize
    );
}

#[test]
fn paving_is_not_approved_as_natural_rock() {
    assert!(
        REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .all(|source| source.role != AssetRole::Rock && source.id != 15018)
    );
    assert!(NATURAL_ROCK_FALLBACK.contains("procedural"));
    assert_eq!(
        OPTIONAL_TERRAIN_SOURCES[0].terrain_topology,
        Some(TerrainFrameTopology::CoordinateStableAccents)
    );
}

#[test]
fn manifest_reference_preserves_the_drs_type_tag() {
    assert_eq!(
        OPTIONAL_RESOURCE_SOURCES[0].manifest_source(),
        "graphics.drs:[32, 112, 108, 115]:4652"
    );
}

#[test]
fn reviewed_resource_sources_preserve_semantic_order_and_frame_counts() {
    assert_eq!(
        OPTIONAL_RESOURCE_SOURCES
            .iter()
            .map(|source| (source.role, source.id, source.frames))
            .collect::<Vec<_>>(),
        vec![
            (AssetRole::WoodTree, 4652, 14),
            (AssetRole::WoodTreeShadow, 2296, 14),
            (AssetRole::ForageBush, 2560, 4),
            (AssetRole::GoldDeposit, 4479, 7),
            (AssetRole::StoneDeposit, 4482, 7),
            (AssetRole::TreeConifer, 4654, 9),
            (AssetRole::TreePalm, 4653, 13),
            (AssetRole::CliffFace, 226, 25),
        ]
    );
}

#[test]
fn every_reviewed_terrain_source_loads_its_complete_periodic_texture_grid() {
    assert!(
        REQUIRED_RENDER_SOURCES
            .iter()
            .filter(|source| matches!(
                source.role,
                AssetRole::TemperateGrass
                    | AssetRole::DryGrass
                    | AssetRole::Dirt
                    | AssetRole::Sand
                    | AssetRole::Water
            ))
            .all(|source| source.terrain_topology
                == Some(TerrainFrameTopology::PeriodicXMajorReversedY {
                    columns: 10,
                    rows: 10
                })
                && source
                    .terrain_topology
                    .unwrap()
                    .supports_frames(source.frames))
    );
}
