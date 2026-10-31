use super::*;

#[test]
fn chopped_trees_follow_each_species_dat_reference_and_use_reviewed_art() {
    for (stage, (unit, full)) in trees::STAGES
        .iter()
        .zip([(349, 4652), (350, 4654), (351, 4653)])
    {
        assert_eq!((stage.unit, stage.full_slp), (unit, full));
        assert_eq!(
            (
                stage.chopped_graphic,
                stage.chopped_slp,
                stage.stump_unit,
                stage.stump_slp
            ),
            (1699, 1252, 415, 1035)
        );
        for id in [stage.full_slp, stage.chopped_slp, stage.stump_slp] {
            assert!(
                OPTIONAL_RESOURCE_SOURCES
                    .iter()
                    .any(|source| source.id == id)
            );
        }
    }
}

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
    // Castle poses, architecture and deduplicated composite recipes extend the catalog.
    // King poses extend the current semantic catalog; atlas limits are unchanged.
    assert_eq!(VERSION, 21);
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
            (AssetRole::VillagerForaging as usize, 2589, 75),
            (AssetRole::VillagerCarryingFood as usize, 2592, 75),
            (AssetRole::VillagerCarryingGold as usize, 2117, 75),
            (AssetRole::VillagerCarryingStone as usize, 1552, 75),
            (AssetRole::VillagerMiningStone as usize, 1560, 75),
            (AssetRole::RallyFlag as usize, 4480, 10),
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
    for id in [2589, 2592, 2117, 1552, 1560] {
        let source = OPTIONAL_RESOURCE_SOURCES
            .iter()
            .find(|source| source.id == id)
            .expect("reviewed villager work/carry source");
        assert_eq!(source.frames, 75);
        assert_eq!(source.archive, "graphics.drs");
    }
}

#[test]
fn rally_flag_uses_the_dat_movement_flag_group() {
    let flag = OPTIONAL_RESOURCE_SOURCES
        .iter()
        .find(|s| s.role == AssetRole::RallyFlag)
        .unwrap();
    assert_eq!(
        (flag.archive, flag.id, flag.frames),
        ("graphics.drs", 4480, 10)
    );
    assert!(flag.terrain_topology.is_none());
}
#[test]
fn bridge_catalog_selects_only_the_six_native_scenario_pieces() {
    let plan = crate::catalog::buildings::supplemental(
        |s| s.starts_with("de/"),
        |_| false,
        |_| false,
        |_| false,
    );
    let bridges: Vec<_> = plan
        .iter()
        .filter(|item| (65..71).contains(&item.slot))
        .collect();
    assert_eq!(bridges.len(), 6);
    for (item, stem) in bridges.iter().zip([
        "s_bridge_a_top",
        "s_bridge_a_middle",
        "s_bridge_a_bottom",
        "s_bridge_b_top",
        "s_bridge_b_middle",
        "s_bridge_b_bottom",
    ]) {
        assert_eq!(item.source, format!("de/{stem}_x1.sld"));
        assert_eq!((item.frames, item.take), (1, 1));
        assert!(item.resident);
        assert!(
            !item.required,
            "missing private art keeps the semantic fallback"
        );
    }
}

#[test]
fn bridge_catalog_prefers_original_slps_when_available() {
    let plan = crate::catalog::buildings::supplemental(|_| true, |_| false, |_| false, |_| false);
    for (item, slp) in plan
        .iter()
        .filter(|item| (65..71).contains(&item.slot))
        .zip([3688, 3690, 3692, 3694, 3696, 3698])
    {
        assert_eq!(
            item.source,
            format!("graphics.drs:[32, 112, 108, 115]:{slp}")
        );
    }
}

#[test]
fn castle_supplemental_slots_keep_distinct_families_and_de_fallback() {
    let plan = crate::catalog::buildings::supplemental(|_| true, |_| false, |_| false, |_| false);
    for (slot, slp) in [(45, 305), (46, 281), (49, 931)] {
        let items: Vec<_> = plan.iter().filter(|item| item.slot == slot).collect();
        assert_eq!(items.len(), 1, "one Castle family per slot {slot}");
        assert_eq!(
            items[0].source,
            format!("graphics.drs:[32, 112, 108, 115]:{slp}")
        );
    }
    let plan = crate::catalog::buildings::supplemental(
        |source| source.starts_with("de/"),
        |_| false,
        |_| false,
        |_| false,
    );
    let castle = plan.iter().find(|item| item.slot == 45).unwrap();
    assert_eq!(castle.source, "de/b_west_castle_age3_x1.sld");
    let rubble = plan.iter().find(|item| item.slot == 49).unwrap();
    assert_eq!(rubble.source, "de/b_west_castle_age3_rubble_x1.sld");
}

#[test]
fn castle_dock_uses_licensed_de_art_when_the_trial_lacks_its_slp() {
    // The DAT binding owns the Dock; completed building categories must not
    // reintroduce a separately maintained importer filename list.
    assert!(crate::catalog::GAMEPLAY_BUILDING_IDS.contains(&47));
    assert!(
        !crate::de::SELECTION
            .iter()
            .any(|item| item.stem == "b_west_dock_age3")
    );
    let plan = crate::catalog::buildings::supplemental(
        |source| source.starts_with("de/"),
        |_| false,
        |_| false,
        |_| false,
    );
    let dock = plan.iter().find(|item| item.slot == 59).unwrap();
    assert_eq!(dock.source, "de/b_west_dock_age3_x1.sld");
    assert_eq!((dock.frames, dock.take), (1, usize::MAX));
}
