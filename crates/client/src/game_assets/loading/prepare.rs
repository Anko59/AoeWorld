//! Synchronous native selection and semantic resolution before streaming awaits.
use super::*;
use std::ops::Range;
pub(super) struct Prepared {
    pub manifest: Manifest,
    pub selected: Vec<usize>,
    pub placements: Vec<Placement>,
    pub maskable: Vec<bool>,
    pub actor_ranges: Vec<Option<Range<usize>>>,
    pub art: GameArt,
    pub native: Option<NativeFrames>,
    pub pages: u32,
}
#[inline(never)]
pub(super) fn prepare(mut manifest: Manifest, team: [u8; 3]) -> Result<Prepared, JsValue> {
    if manifest.version != 1 {
        return Err(JsValue::from_str("Unsupported asset pack version"));
    }
    manifest.buildings.retain(|b| {
        aoe_assets::catalog::GAMEPLAY_BUILDING_IDS.contains(&b.unit_id)
            || aoe_assets::catalog::ENVIRONMENT_UNIT_IDS.contains(&b.unit_id)
    });
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
            if !manifest.terrains.is_empty() && selection.terrain_topology.is_some() {
                continue;
            }
            let environment = manifest
                .buildings
                .iter()
                .any(|b| aoe_assets::catalog::ENVIRONMENT_UNIT_IDS.contains(&b.unit_id));
            if environment
                && matches!(
                    selection.role,
                    AssetRole::WoodTree
                        | AssetRole::WoodTreeShadow
                        | AssetRole::ForageBush
                        | AssetRole::GoldDeposit
                        | AssetRole::StoneDeposit
                        | AssetRole::TreeConifer
                        | AssetRole::TreePalm
                        | AssetRole::RockPile
                        | AssetRole::DeadwoodStump
                        | AssetRole::DeadwoodLog
                        | AssetRole::DeadwoodTree
                        | AssetRole::CliffFace
                )
            {
                continue;
            }
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
    let native_farm = terrain::selection(
        &manifest,
        &mut selected,
        &mut domains,
        &mut ranges,
        &mut terrain_topology,
        &mut depth_topology,
    )?;
    let rally_range = building_sprites::rally(&manifest, &mut selected, &mut domains)?;
    let actor_ranges = actors::select(&manifest, &mut selected, &mut domains)?;
    let building_start = selected.len();
    buildings::select(&manifest, &mut selected, &mut domains)?;
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
    atlas_budget(selected.len());
    let art = art::resolve(art::Input {
        manifest: &manifest,
        selected: &selected,
        records: &records,
        ranges: &ranges,
        actor_ranges: &actor_ranges,
        rally_range,
        native_farm,
        terrain_topology,
        depth_topology,
        residents: native.as_ref().map_or(0, NativeFrames::len),
    })?;

    Ok(Prepared {
        manifest,
        selected,
        placements,
        maskable,
        actor_ranges,
        art,
        native,
        pages,
    })
}
