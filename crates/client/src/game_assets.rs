//! Loads only the local pack pages needed by the first map and compacts them.
use aoe_assets::catalog::{
    AssetRole, TerrainFrameTopology,
    packing::{AtlasDomain, FrameExtent, MAX_SELECTED_FRAMES, Placement, pack_frames},
    runtime::{
        OPTIONAL_RESOURCES as OPTIONAL_RESOURCE_SOURCES,
        OPTIONAL_TERRAIN as OPTIONAL_TERRAIN_SOURCES, REQUIRED as REQUIRED_RENDER_SOURCES,
    },
};
#[path = "game_assets/manifest.rs"]
mod manifest;
use aoe_rendering::{
    AtlasAddress, GAME_ATLAS_BYTES, GAME_ATLAS_PAGE_BYTES, GAME_ATLAS_SIDE, GameArt, GameFrame,
    TerrainTopology,
};
#[path = "game_assets/placement.rs"]
mod placement;
#[cfg(test)]
#[path = "game_assets/tests/selection.rs"]
mod selection_tests;
use js_sys::Uint8Array;
use manifest::Manifest;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

const ROLE_SLOTS: usize = AssetRole::TreePalm as usize + 1;
type RoleRanges = [Option<std::ops::Range<usize>>; ROLE_SLOTS];

async fn fetch(path: &str) -> Result<Vec<u8>, JsValue> {
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
    if name.contains('/') || name.contains('\\') || !name.ends_with(".png") {
        return Err(JsValue::from_str("Invalid atlas page name"));
    }
    let bytes = fetch(&format!("/asset-pack/{name}")).await?;
    crate::png_page::decode(bytes).await
}

fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

// Startup-only lists are bounded by reviewed catalogue frame counts (678 total).
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

fn source_present(manifest: &Manifest, source: &str) -> bool {
    manifest.frames.iter().any(|frame| frame.source == source)
}

const TERRAIN_ROLES: [AssetRole; 7] = [
    AssetRole::TemperateGrass,
    AssetRole::DryGrass,
    AssetRole::Dirt,
    AssetRole::Sand,
    AssetRole::Rock,
    AssetRole::Water,
    AssetRole::ForestFloor,
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

pub async fn load() -> Result<(GameArt, Vec<u8>), JsValue> {
    let bytes = fetch("/asset-pack/manifest.json").await?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(error)?;
    // The DTO owns its used strings; release raw JSON before PNG decode awaits.
    drop(bytes);
    if manifest.version != 1 {
        return Err(JsValue::from_str("Unsupported asset pack version"));
    }
    let mut selected = Vec::new();
    let mut domains = Vec::new();
    let mut terrain_topology = [None; 7];
    // Catalog roles are bounded, not a dynamic startup search tree. The browser
    // oracle covers every selected role and the intentionally absent rock role.
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
            if frames.len() > MAX_SELECTED_FRAMES - selected.len() {
                return Err(JsValue::from_str("Too many selected frames"));
            }
            let terrain_slot = TERRAIN_ROLES
                .iter()
                .position(|role| *role == selection.role);
            let domain = if let Some(slot) = terrain_slot {
                terrain_topology[slot] = selection.terrain_topology.map(renderer_topology);
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
    // Validate count, source bounds, and all extents before pixel allocation.
    let placements = placement::plan(&manifest, &selected, &domains)?;
    drop(domains);
    let records = selected
        .iter()
        .zip(&placements)
        .map(|(&index, &placed)| placement::record(&manifest.frames[index], placed))
        .collect::<Vec<_>>();
    let mut pixels = vec![0; GAME_ATLAS_BYTES];
    // Every origin is reserved; explicit renderer white addressing uses page 2.
    for origin in (0..GAME_ATLAS_BYTES).step_by(GAME_ATLAS_PAGE_BYTES) {
        pixels[origin..origin + 4].copy_from_slice(&[255; 4]);
    }
    // A bounded index vector groups source pages without a map or page-ID-sized
    // allocation. Grouping is for decoding only; records stay in semantic order.
    let source_order = source_order(&manifest, &selected);
    let mut start = 0;
    while start < source_order.len() {
        let source_page = manifest.frames[selected[source_order[start]]].page;
        let atlas = &manifest.pages[usize::from(source_page)];
        let color = page(&atlas.color).await?;
        let player = page(&atlas.player).await?;
        let shadow = page(&atlas.shadow).await?;
        let mut end = start;
        while end < source_order.len() {
            let semantic = source_order[end];
            let frame = &manifest.frames[selected[semantic]];
            if frame.page != source_page {
                break;
            }
            placement::copy(
                frame,
                placements[semantic],
                &color,
                &player,
                &shadow,
                &mut pixels,
            )?;
            end += 1;
        }
        start = end;
    }
    let group = |role: AssetRole| {
        ranges
            .get(role as usize)
            .and_then(Option::as_ref)
            .map(|range| records[range.clone()].to_vec())
            .unwrap_or_default()
    };
    let terrain = TERRAIN_ROLES.map(group);
    Ok((
        GameArt {
            walking: group(AssetRole::CavalryWalking),
            standing: group(AssetRole::CavalryStanding),
            grass: terrain[0].clone(),
            terrain,
            terrain_topology,
            resources: [
                group(AssetRole::ForageBush),
                group(AssetRole::WoodTree),
                group(AssetRole::GoldDeposit),
                group(AssetRole::StoneDeposit),
            ],
            tree_shadows: group(AssetRole::WoodTreeShadow),
            tree_families: [group(AssetRole::TreeConifer), group(AssetRole::TreePalm)],
        },
        pixels,
    ))
}
