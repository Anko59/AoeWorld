use super::*;

#[test]
fn mountain_snow_source_is_ground_art_instead_of_cracked_ice() {
    let source = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(OPTIONAL_TERRAIN_SOURCES.iter())
        .find(|source| source.role == AssetRole::Snow)
        .expect("native snow ground source");
    assert_ne!(
        source.id, 15024,
        "15024 is cracked ice, not soft snow ground"
    );
    assert!(
        source
            .terrain_topology
            .is_some_and(|topology| topology.supports_frames(source.frames))
    );
}

#[test]
fn mountain_rock_has_a_native_ground_source_separate_from_gravel_and_paving() {
    let source = REQUIRED_RENDER_SOURCES
        .iter()
        .chain(OPTIONAL_TERRAIN_SOURCES.iter())
        .find(|source| source.role == AssetRole::Rock)
        .expect("native rock ground source");
    assert!(
        ![15017, 15018].contains(&source.id),
        "gravel and paving are not mountain rock ground"
    );
    assert!(
        source
            .terrain_topology
            .is_some_and(|topology| topology.supports_frames(source.frames))
    );
}

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
    assert_eq!(VERSION, 15);
    assert_eq!(AssetRole::StoneDeposit as usize, 13);
    assert_eq!(AssetRole::TreeConifer as usize, 14);
    assert_eq!(AssetRole::TreePalm as usize, 15);
    assert_eq!(AssetRole::Gravel as usize, 22);
    assert_eq!(AssetRole::Snow as usize, 24);
    assert_eq!(OPTIONAL_TERRAIN_SOURCES[8].id, 15033);
    assert_eq!(OPTIONAL_TERRAIN_SOURCES[9].id, 15024);
    assert_eq!(
        OPTIONAL_TERRAIN_SOURCES[4..8]
            .iter()
            .map(|source| (source.role as usize, source.id, source.frames))
            .collect::<Vec<_>>(),
        vec![
            (AssetRole::GrassDirt as usize, 15006, 100),
            (AssetRole::WornGrass as usize, 15009, 100),
            (AssetRole::LushGrass as usize, 15001, 100),
            (AssetRole::Gravel as usize, 15017, 100),
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
            .all(|source| source.id != 15018)
    );
    assert!(NATURAL_ROCK_FALLBACK.contains("native rock"));
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
            .map(|source| (source.role as usize, source.id, source.frames))
            .collect::<Vec<_>>(),
        vec![
            (AssetRole::WoodTree as usize, 4652, 14),
            (AssetRole::WoodTreeShadow as usize, 2296, 14),
            (AssetRole::ForageBush as usize, 2560, 4),
            (AssetRole::GoldDeposit as usize, 4479, 7),
            (AssetRole::StoneDeposit as usize, 4482, 7),
            (AssetRole::TreeConifer as usize, 4654, 9),
            (AssetRole::TreePalm as usize, 4653, 13),
            (AssetRole::CliffFace as usize, 226, 25),
            (AssetRole::RockPile as usize, 1034, 7),
            (AssetRole::Shrub as usize, 4474, 6),
            (AssetRole::DeadwoodStump as usize, 1035, 3),
            (AssetRole::DeadwoodLog as usize, 1252, 2),
            (AssetRole::DeadwoodTree as usize, 1262, 1),
            (AssetRole::VillagerStanding as usize, 1479, 75),
            (AssetRole::VillagerWalking as usize, 1484, 75),
            (AssetRole::VillagerChopping as usize, 1535, 75),
            (AssetRole::VillagerCarryingWood as usize, 1536, 75),
            (AssetRole::VillagerForaging as usize, 1490, 75),
            (AssetRole::VillagerCarryingFood as usize, 1493, 75),
            (AssetRole::VillagerMiningGold as usize, 1545, 75),
            (AssetRole::VillagerCarryingOre as usize, 1552, 75),
            (AssetRole::VillagerMiningStone as usize, 1560, 75),
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

#[test]
fn village_art_is_optional_and_uses_reviewed_directional_prefixes() {
    for (id, count) in [(1479, 75), (1484, 75), (1535, 75), (1536, 75)] {
        let source = OPTIONAL_RESOURCE_SOURCES
            .iter()
            .find(|source| source.id == id)
            .expect("optional village sprite metadata");
        assert_eq!(source.frames, count);
        assert!(source.terrain_topology.is_none());
        assert!(
            !REQUIRED_RENDER_SOURCES
                .iter()
                .any(|required| required.id == id)
        );
    }
}

#[test]
fn resource_specific_worker_sets_are_selected_from_the_local_pack() {
    for id in [1490, 1493, 1545, 1552, 1560] {
        let source = OPTIONAL_RESOURCE_SOURCES
            .iter()
            .find(|source| source.id == id)
            .expect("reviewed villager work/carry source");
        assert_eq!(source.frames, 75);
        assert_eq!(source.archive, "graphics.drs");
    }
}
