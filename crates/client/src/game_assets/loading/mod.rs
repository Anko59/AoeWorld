//! Startup selection and static uploads beside the native animation cache.
use super::*;

#[inline(never)]
pub(crate) async fn load(
    sink: &mut dyn AtlasSink,
) -> Result<(GameArt, [u8; 3], Option<NativeFrames>), JsValue> {
    let team = team_colour();
    let bytes = fetch("/asset-pack/manifest.json").await?;
    let mut manifest = decode_manifest(&bytes)?;
    manifest.buildings =
        recipes::decode(&manifest.building_data, &manifest.frames).map_err(error)?;
    building_sprites::overlay(&mut manifest).await?;
    // The DTO owns its used strings; release raw JSON before PNG decode awaits.
    drop(bytes);
    if manifest.version != 1 {
        return Err(JsValue::from_str("Unsupported asset pack version"));
    }
    manifest
        .buildings
        .retain(|b| aoe_assets::catalog::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id));
    let mut selected = Vec::new();
    let mut domains = Vec::new();
    let mut terrain_topology = [None; 12];
    let mut depth_topology = [None; 4];
    // Catalog roles are bounded, not a dynamic startup search tree. The browser
    // oracle covers every selected role.
    let mut ranges: RoleRanges = std::array::from_fn(|_| None);
    for (sources, required) in [
        (REQUIRED_RENDER_SOURCES.as_slice(), true),
        (OPTIONAL_RESOURCE_SOURCES.as_slice(), false),
        (OPTIONAL_TERRAIN_SOURCES.as_slice(), false),
    ] {
        for selection in sources {
            let source = selection.manifest_source();
            let Some(frames) = source_frames(&manifest, &source, selection.frames) else {
                // Optional means absent art may use a semantic fallback. A
                // present but incomplete/duplicate source is corrupt, not absent.
                if required || source_present(&manifest, &source) {
                    return Err(JsValue::from_str(&format!(
                        "Local pack has missing or corrupt reviewed art ({source})"
                    )));
                }
                continue;
            };
            let frames = compact_standing(frames, selection.role);
            if frames.len() > MAX_SELECTED_FRAMES - selected.len() {
                return Err(JsValue::from_str("Too many selected frames"));
            }
            let terrain_slot = TERRAIN_ROLES
                .iter()
                .position(|role| *role == selection.role);
            let depth_slot = WATER_DEPTH_ROLES
                .iter()
                .position(|role| *role == selection.role);
            if let Some(slot) = terrain_slot {
                terrain_topology[slot] = selection.terrain_topology.map(renderer_topology);
            } else if let Some(slot) = depth_slot {
                depth_topology[slot] = selection.terrain_topology.map(renderer_topology);
            }
            // Match the private packing proof: reviewed topology owns the
            // domain, even for selected sheets with no displayed role (marsh).
            let domain = if selection.terrain_topology.is_some() {
                AtlasDomain::Terrain
            } else {
                AtlasDomain::Objects
            };
            domains.extend(std::iter::repeat_n(domain, frames.len()));
            let start = selected.len();
            selected.extend(frames);
            ranges[selection.role as usize] = Some(start..selected.len());
        }
    }
    let rally_range = building_sprites::rally(&manifest, &mut selected, &mut domains)?;
    let actor_ranges = actors::select(&manifest, &mut selected, &mut domains)?;
    let building_start = selected.len();
    buildings::select(&manifest, &mut selected, &mut domains)?;
    let building_ranges = building_sprites::select(&manifest, &mut selected, &mut domains)?;
    building_sprites::crop_farm(&mut manifest, &selected, &building_ranges)?;
    let fishing_ranges = fishing::select(&manifest, &mut selected, &mut domains)?;
    let mut maskable = vec![false; selected.len()];
    for role in aoe_assets::catalog::runtime::PLAYER_ROLES {
        if let Some(range) = &ranges[role as usize] {
            maskable[range.clone()].fill(true);
        }
    }
    for (index, range) in actor_ranges.iter().enumerate() {
        if aoe_assets::catalog::runtime::actor_maskable(index)
            && let Some(range) = range
        {
            maskable[range.clone()].fill(true);
        }
    }
    for index in building_start..maskable.len() {
        maskable[index] = domains[index] == AtlasDomain::Objects;
    }
    for (domain, owned) in domains.iter_mut().zip(&maskable) {
        if *domain == AtlasDomain::Objects && !owned {
            *domain = AtlasDomain::Shared;
        }
    }
    // Validate every source/extent before pixel allocation. Shared art has no
    // team mask and may use unused terrain space inside the same five pages.
    let placements = placement::plan(&manifest, &selected, &domains)?;
    drop(domains);
    let mut records = selected
        .iter()
        .zip(&placements)
        .map(|(&index, &placed)| placement::record(&manifest.frames[index], placed))
        .collect::<Vec<_>>();
    let native = native::prepare(
        &manifest,
        &selected,
        &maskable,
        &actor_ranges,
        team,
        &placements,
        &mut records,
    )?;
    let pages = native.as_ref().map_or_else(
        || placement::pages(&placements),
        |native| u32::from(native.layer_count()),
    );
    let group = |role| group_frames(&ranges, &records, role);
    let mut terrain: [Vec<GameFrame>; 12] = Default::default();
    actors::fill_groups(&mut terrain, &TERRAIN_ROLES, &ranges, &records);
    let mut village: [Vec<GameFrame>; 9] = Default::default();
    actors::fill_groups(
        &mut village,
        &[
            AssetRole::VillagerStanding,
            AssetRole::VillagerWalking,
            AssetRole::VillagerChopping,
            AssetRole::VillagerCarryingWood,
            AssetRole::VillagerForaging,
            AssetRole::VillagerCarryingFood,
            AssetRole::VillagerCarryingGold,
            AssetRole::VillagerCarryingStone,
            AssetRole::VillagerMiningStone,
        ],
        &ranges,
        &records,
    );
    let buildings = buildings::resolve(&manifest, &selected, &records);
    let foundation = buildings::foundation(&manifest, &selected, &records)?;

    atlas_budget(selected.len());
    let art = GameArt {
        resident: vec![None; native.as_ref().map_or(0, NativeFrames::len)],
        building_clocks: Vec::new(),
        actor_rows: crate::building_ui::content::sprites()
            .iter()
            .map(|s| s.rows())
            .collect(),
        actors: actors::resolve(&manifest, &selected, &records, &actor_ranges),
        fishing: std::array::from_fn(|i| {
            fishing_ranges[i]
                .as_ref()
                .map_or_else(Vec::new, |r| records[r.clone()].to_vec())
        }),
        walking: group(AssetRole::CavalryWalking),
        standing: group(AssetRole::CavalryStanding),
        village,
        compositions: buildings,
        buildings: std::array::from_fn(|i| {
            let mut frames = building_ranges[i]
                .as_ref()
                .map_or_else(Vec::new, |r| records[r.clone()].to_vec());
            // Scenario pieces span two source tiles. Half scale gives the
            // buildable single-lane deck its one-tile cadence and width.
            if (65..=70).contains(&i) {
                for frame in &mut frames {
                    frame.size = frame.size.map(|n| n * 0.5);
                    frame.anchor = frame.anchor.map(|n| n * 0.5);
                }
            }
            frames
        }),
        foundation,
        grass: terrain[0].clone(),
        terrain,
        terrain_topology,
        water_depth: std::array::from_fn(|slot| {
            (group(WATER_DEPTH_ROLES[slot]), depth_topology[slot])
        }),
        resources: [
            group(AssetRole::ForageBush),
            group(AssetRole::WoodTree),
            group(AssetRole::GoldDeposit),
            group(AssetRole::StoneDeposit),
        ],
        tree_shadows: group(AssetRole::WoodTreeShadow),
        tree_families: [group(AssetRole::TreeConifer), group(AssetRole::TreePalm)],
        scenery: aoe_rendering::SceneryArt {
            rally_flags: building_sprites::rally_frames(
                &records,
                rally_range,
                group(AssetRole::RallyFlag),
            ),
            cliff_faces: group(AssetRole::CliffFace),
            rocks: group(AssetRole::RockPile),
            shrubs: group(AssetRole::Shrub),
            stumps: group(AssetRole::DeadwoodStump),
            snags: group(AssetRole::DeadwoodTree),
            logs: group(AssetRole::DeadwoodLog),
        },
    };

    streaming::Input {
        pages,
        manifest: &manifest,
        selected: &selected,
        placements: &placements,
        maskable: &maskable,
        actor_ranges: &actor_ranges,
        art: &art,
        team,
    }
    .upload(sink)
    .await?;
    Ok((art, team, native))
}
