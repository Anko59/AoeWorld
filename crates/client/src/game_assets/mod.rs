//! Loads only the local pack pages needed by the first map and compacts them.
use aoe_assets::catalog::{
    AssetRole, TerrainFrameTopology,
    packing::{AtlasDomain, FrameExtent, MAX_SELECTED_FRAMES, Placement, pack_frames},
    runtime::{
        OPTIONAL_RESOURCES as OPTIONAL_RESOURCE_SOURCES,
        OPTIONAL_TERRAIN as OPTIONAL_TERRAIN_SOURCES, REQUIRED as REQUIRED_RENDER_SOURCES,
    },
};
#[cfg(test)]
#[path = "tests/actors.rs"]
mod actor_tests;
mod actors;
mod building_sprites;
mod buildings;
mod fishing;
#[path = "manifest.rs"]
mod manifest;
#[cfg(test)]
#[path = "tests/player_masks.rs"]
mod player_mask_tests;
mod resident;
mod sink;
mod streaming;
pub use sink::RendererSink;
#[cfg(test)]
#[path = "tests/collected.rs"]
mod collected;
#[cfg(test)]
pub(crate) use collected::CollectedAtlas;
#[cfg(test)]
#[path = "tests/resident.rs"]
mod resident_tests;
use aoe_rendering::{
    AtlasAddress, GAME_ATLAS_BYTES, GAME_ATLAS_PAGE_BYTES, GAME_ATLAS_PAGES, GAME_ATLAS_SIDE,
    GameArt, GameFrame, TerrainTopology,
};
pub(crate) use manifest::{decode_bounds, decode_chunk, decode_chunks};
#[cfg(test)]
#[path = "tests/building_sprites.rs"]
mod building_sprite_tests;
#[cfg(test)]
#[path = "tests/buildings.rs"]
mod building_tests;
#[cfg(test)]
#[path = "tests/fishing.rs"]
mod fishing_tests;
#[path = "placement.rs"]
mod placement;
#[path = "recipes.rs"]
mod recipes;
#[cfg(test)]
#[path = "tests/selection.rs"]
mod selection_tests;
use js_sys::Uint8Array;
use manifest::Manifest;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

const ROLE_SLOTS: usize = AssetRole::RallyFlag as usize + 1;
type RoleRanges = [Option<std::ops::Range<usize>>; ROLE_SLOTS];

pub(crate) async fn fetch(path: &str) -> Result<Vec<u8>, JsValue> {
    let window = web_sys::window().ok_or("No window")?;
    let response: Response = JsFuture::from(window.fetch_with_str(path))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(
            "AoE II assets are unavailable. Start AoeWorld with a local asset pack.",
        ));
    }
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    Ok(Uint8Array::new(&bytes).to_vec())
}

async fn page(name: &str) -> Result<Vec<u8>, JsValue> {
    if name
        .strip_prefix("buildings/")
        .unwrap_or(name)
        .contains('/')
        || name.contains('\\')
        || !name.ends_with(".png")
    {
        return Err(JsValue::from_str("Invalid atlas page name"));
    }
    let bytes = fetch(&format!("/asset-pack/{name}")).await?;
    crate::png_page::decode(bytes).await
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

// Both the base pack and DE overlay share one JSON parser instantiation.
#[inline(never)]
fn decode_manifest(bytes: &[u8]) -> Result<Manifest, JsValue> {
    manifest::decode(bytes)
}

// Startup-only lists are bounded by reviewed catalogue frame counts (`MAX_REVIEWED_FRAMES`).
// Insertion sorting avoids generic quicksort code for these small lists; do not
// reuse this helper for unbounded or per-frame data.
#[inline(never)]
fn sort_indices(
    indices: &mut [usize],
    compare: &mut dyn FnMut(&usize, &usize) -> std::cmp::Ordering,
) {
    for at in 1..indices.len() {
        let value = indices[at];
        let mut slot = at;
        while slot > 0 && compare(&value, &indices[slot - 1]).is_lt() {
            indices[slot] = indices[slot - 1];
            slot -= 1;
        }
        indices[slot] = value;
    }
}

#[inline(never)]
fn source_frames(manifest: &Manifest, source: &str, count: u32) -> Option<Vec<usize>> {
    if count as usize > MAX_SELECTED_FRAMES {
        return None;
    }
    let mut frames = Vec::new();
    for (index, frame) in manifest.frames.iter().enumerate() {
        if frame.source == source && frame.frame < count {
            if frames.len() == count as usize {
                return None;
            }
            frames.push(index);
        }
    }
    // Reject oversized and incomplete source groups before quadratic sorting.
    if frames.len() != count as usize {
        return None;
    }
    sort_indices(&mut frames, &mut |left, right| {
        manifest.frames[*left]
            .frame
            .cmp(&manifest.frames[*right].frame)
    });
    frames
        .iter()
        .enumerate()
        .all(|(i, index)| manifest.frames[*index].frame == i as u32)
        .then_some(frames)
}

#[inline(never)]
fn source_present(manifest: &Manifest, source: &str) -> bool {
    manifest.frames.iter().any(|frame| frame.source == source)
}

// Idle actors already display only the first cell of each facing row. Drop
// unused cells after validating the original prefix; walking/work rows stay full.
#[inline(never)]
fn compact_standing(frames: Vec<usize>, role: AssetRole) -> Vec<usize> {
    let step = match role {
        AssetRole::CavalryStanding => 10,
        AssetRole::VillagerStanding => 15,
        _ => return frames,
    };
    frames.into_iter().step_by(step).collect()
}

const TERRAIN_ROLES: [AssetRole; 12] = [
    AssetRole::TemperateGrass,
    AssetRole::DryGrass,
    AssetRole::Dirt,
    AssetRole::Sand,
    AssetRole::Rock,
    AssetRole::Water,
    AssetRole::ForestFloor,
    AssetRole::GrassDirt,
    AssetRole::WornGrass,
    AssetRole::LushGrass,
    AssetRole::Gravel,
    AssetRole::Snow,
];

/// Optional water sheets in renderer order: medium, deep, frozen, wadeable marsh.
const WATER_DEPTH_ROLES: [AssetRole; 4] = [
    AssetRole::WaterMedium,
    AssetRole::WaterDeep,
    AssetRole::Ice,
    AssetRole::Shallows,
];

fn renderer_topology(topology: TerrainFrameTopology) -> TerrainTopology {
    match topology {
        TerrainFrameTopology::PeriodicXMajorReversedY { columns, rows } => {
            TerrainTopology::PeriodicXMajorReversedY { columns, rows }
        }
        TerrainFrameTopology::CoordinateStableAccents => TerrainTopology::CoordinateStableAccents,
    }
}

// Called only after plan validates the bounded semantic selection.
fn source_order(manifest: &Manifest, selected: &[usize]) -> Vec<usize> {
    let mut order = (0..selected.len()).collect::<Vec<_>>();
    sort_indices(&mut order, &mut |left, right| {
        manifest.frames[selected[*left]]
            .page
            .cmp(&manifest.frames[selected[*right]].page)
    });
    order
}

// Aliased masked cells must be uploaded once: repeated coverage composition
// would change their alpha. Shared cells deliberately retain distinct places.
fn copy_order(manifest: &Manifest, selected: &[usize], placements: &[Placement]) -> Vec<usize> {
    let mut order = source_order(manifest, selected);
    order.retain(|&semantic| !placements[..semantic].contains(&placements[semantic]));
    order
}

// Share startup lookup/copy code instead of specializing it for every role.
#[inline(never)]
fn group_frames(ranges: &RoleRanges, records: &[GameFrame], role: AssetRole) -> Vec<GameFrame> {
    match &ranges[role as usize] {
        Some(range) => records[range.clone()].to_vec(),
        None => Vec::new(),
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(
    inline_js = "export function atlas_budget(count) { document.getElementById('playground')?.setAttribute('data-atlas-frames',String(count)); } export function selected_team_colour() { const c={blue:[65,145,245],red:[245,65,65],green:[50,190,65],yellow:[245,220,40],cyan:[40,220,220],purple:[175,65,220],grey:[160,160,160],orange:[245,145,40]};return new Uint8Array(c[document.getElementById('playground')?.dataset.playerColour]??c.blue); }"
)]
extern "C" {
    fn atlas_budget(count: usize);
    fn selected_team_colour() -> Vec<u8>;
}

pub(crate) fn team_colour() -> [u8; 3] {
    let colour = selected_team_colour();
    [colour[0], colour[1], colour[2]]
}

/// GPU sinks receive one finished page at a time. CPU sinks can expose the
/// atlas they already retain, filled in source order before `finish`.
pub trait AtlasSink {
    /// Existing CPU destination storage; page-only sinks allocate no full atlas.
    fn resident_buffer(&mut self) -> Result<Option<&mut [u8]>, String> {
        Ok(None)
    }
    /// The zeroed buffer for page `index`; the loader fills it in place.
    fn page_buffer(&mut self, index: u32) -> Result<&mut [u8], String>;
    /// Hands the filled page over.
    fn commit(&mut self, index: u32) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String>;
}

/// Loads art and the whole atlas for tests that inspect packed texels.
#[cfg(test)]
pub(crate) async fn load_collected() -> Result<(GameArt, Vec<u8>, [u8; 3]), JsValue> {
    let mut atlas = CollectedAtlas(Vec::new());
    let (art, team) = load(&mut atlas).await?;
    Ok((art, atlas.0, team))
}

#[inline(never)]
pub async fn load(sink: &mut dyn AtlasSink) -> Result<(GameArt, [u8; 3]), JsValue> {
    let team = team_colour();
    let bytes = fetch("/asset-pack/manifest.json").await?;
    let mut manifest = decode_manifest(&bytes)?;
    building_sprites::overlay(&mut manifest).await?;
    // The DTO owns its used strings; release raw JSON before PNG decode awaits.
    drop(bytes);
    if manifest.version != 1 {
        return Err(JsValue::from_str("Unsupported asset pack version"));
    }
    manifest.buildings =
        recipes::decode(&manifest.building_data, &manifest.frames).map_err(error)?;
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
    let records = selected
        .iter()
        .zip(&placements)
        .map(|(&index, &placed)| placement::record(&manifest.frames[index], placed))
        .collect::<Vec<_>>();
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
    Ok((art, team))
}
