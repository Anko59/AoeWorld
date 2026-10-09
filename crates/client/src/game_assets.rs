//! Loads only the local pack pages needed by the first map and compacts them.
use aoe_assets::catalog::{
    AssetRole,
    runtime::{
        OPTIONAL_RESOURCES as OPTIONAL_RESOURCE_SOURCES,
        OPTIONAL_TERRAIN as OPTIONAL_TERRAIN_SOURCES, REQUIRED as REQUIRED_RENDER_SOURCES,
    },
};
#[path = "game_assets/manifest.rs"]
mod manifest;
use aoe_rendering::{GAME_ATLAS_SIDE, GameArt, GameFrame};
use js_sys::Uint8Array;
use manifest::Manifest;
use std::collections::BTreeMap;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

const ROLE_SLOTS: usize = AssetRole::StoneDeposit as usize + 1;
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

// Startup-only lists are bounded by reviewed catalogue frame counts (656 total).
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
    let mut frames = Vec::new();
    for (index, frame) in manifest.frames.iter().enumerate() {
        if frame.source == source && frame.frame < count {
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

fn packing_order(manifest: &Manifest, selected: &[usize]) -> Vec<usize> {
    let mut order = (0..selected.len()).collect::<Vec<_>>();
    // An explicit semantic index keeps the former stable order on size ties.
    sort_indices(&mut order, &mut |left, right| {
        let left_frame = &manifest.frames[selected[*left]];
        let right_frame = &manifest.frames[selected[*right]];
        // Both dimensions are u16: this key preserves height/width ordering exactly.
        let left_size = (u32::from(left_frame.height) << 16) | u32::from(left_frame.width);
        let right_size = (u32::from(right_frame.height) << 16) | u32::from(right_frame.width);
        right_size.cmp(&left_size).then(left.cmp(right))
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
            let start = selected.len();
            selected.extend(frames);
            ranges[selection.role as usize] = Some(start..selected.len());
        }
    }
    let side = GAME_ATLAS_SIDE as usize;
    let mut pixels = vec![0; side * side * 4];
    pixels[..4].copy_from_slice(&[255; 4]);
    // Pack tall sprites first to avoid wasting a row's height on short terrain.
    // Keep semantic frame indices independent from physical atlas placement.
    let mut records = vec![
        GameFrame {
            uv: [0.0; 4],
            size: [0.0; 2],
            anchor: [0.0; 2],
        };
        selected.len()
    ];
    let placement_order = packing_order(&manifest, &selected);
    let mut placements = BTreeMap::<u16, Vec<(usize, usize, usize)>>::new();
    let (mut x, mut y, mut row_height) = (2, 2, 0);
    for index in placement_order {
        let manifest_index = selected[index];
        let f = &manifest.frames[manifest_index];
        let (w, h) = (f.width as usize, f.height as usize);
        if w == 0
            || h == 0
            || w >= side
            || h >= side
            || usize::from(f.x) + w > side
            || usize::from(f.y) + h > side
        {
            return Err(JsValue::from_str("Invalid sprite bounds"));
        }
        if x + w + 1 >= side {
            x = 2;
            y += row_height + 1;
            row_height = 0;
        }
        if y + h + 1 >= side {
            return Err(JsValue::from_str("Game atlas is full"));
        }
        records[index] = GameFrame {
            uv: [
                x as f32 / side as f32,
                y as f32 / side as f32,
                w as f32 / side as f32,
                h as f32 / side as f32,
            ],
            size: [w as f32, h as f32],
            anchor: [f.anchor_x as f32, f.anchor_y as f32],
        };
        placements
            .entry(f.page)
            .or_default()
            .push((manifest_index, x, y));
        x += w + 1;
        row_height = row_height.max(h);
    }
    for (index, frames) in placements {
        let atlas = manifest
            .pages
            .get(index as usize)
            .ok_or("Invalid sprite page")?;
        let color = page(&atlas.color).await?;
        let player = page(&atlas.player).await?;
        let shadow = page(&atlas.shadow).await?;
        for (manifest_index, x, y) in frames {
            let frame = &manifest.frames[manifest_index];
            for row in 0..usize::from(frame.height) {
                for col in 0..usize::from(frame.width) {
                    let source =
                        ((usize::from(frame.y) + row) * side + usize::from(frame.x) + col) * 4;
                    let output = ((y + row) * side + x + col) * 4;
                    let value = if player[source + 3] > 0 {
                        let shade = 0.65 + f32::from(player[source].min(7)) / 7.0 * 0.35;
                        [
                            (65.0 * shade) as u8,
                            (145.0 * shade) as u8,
                            (245.0 * shade) as u8,
                            255,
                        ]
                    } else if color[source + 3] > 0 {
                        color[source..source + 4].try_into().map_err(error)?
                    } else {
                        shadow[source..source + 4].try_into().map_err(error)?
                    };
                    pixels[output..output + 4].copy_from_slice(&value);
                }
            }
        }
    }
    let group = |role: AssetRole| {
        ranges
            .get(role as usize)
            .and_then(Option::as_ref)
            .map(|range| records[range.clone()].to_vec())
            .unwrap_or_default()
    };
    let terrain = [
        group(AssetRole::TemperateGrass),
        group(AssetRole::DryGrass),
        group(AssetRole::Dirt),
        group(AssetRole::Sand),
        group(AssetRole::Rock),
        group(AssetRole::Water),
        group(AssetRole::ForestFloor),
    ];
    Ok((
        GameArt {
            walking: group(AssetRole::CavalryWalking),
            standing: group(AssetRole::CavalryStanding),
            grass: terrain[0].clone(),
            terrain,
            resources: [
                group(AssetRole::ForageBush),
                group(AssetRole::WoodTree),
                group(AssetRole::GoldDeposit),
                group(AssetRole::StoneDeposit),
            ],
            tree_shadows: group(AssetRole::WoodTreeShadow),
        },
        pixels,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn bounded_role_ranges_preserve_reference_lookup_absence_and_replacement() {
        let mut ranges: RoleRanges = std::array::from_fn(|_| None);
        let mut reference = BTreeMap::new();
        let sources = REQUIRED_RENDER_SOURCES
            .iter()
            .chain(&OPTIONAL_RESOURCE_SOURCES)
            .chain(&OPTIONAL_TERRAIN_SOURCES);
        for source in sources.clone() {
            assert!((source.role as usize) < ROLE_SLOTS);
            assert_eq!(ranges[source.role as usize], None);
            for range in [0..source.frames as usize, 2..source.frames as usize + 2] {
                reference.insert(source.role, range.clone());
                ranges[source.role as usize] = Some(range);
                assert_eq!(
                    ranges[source.role as usize].as_ref(),
                    reference.get(&source.role)
                );
            }
        }
        for role in sources.map(|source| source.role).chain([AssetRole::Rock]) {
            assert_eq!(
                ranges.get(role as usize).and_then(Option::as_ref),
                reference.get(&role)
            );
        }
        assert_eq!(ranges[AssetRole::Rock as usize], None);
    }

    #[wasm_bindgen_test]
    fn optional_source_absence_is_distinct_from_corrupt_present_frames() {
        let manifest: Manifest = serde_json::from_value(manifest::fixture()).unwrap();
        assert!(!source_present(&manifest, "absent"));
        assert_eq!(source_frames(&manifest, "absent", 2), None);
        assert!(source_present(&manifest, "fixture"));
        assert_eq!(source_frames(&manifest, "fixture", 2), None);
    }

    #[wasm_bindgen_test]
    fn source_indices_preserve_frame_order_and_reject_missing_or_duplicate_frames() {
        let mut value = manifest::fixture();
        let first = value["frames"][0].clone();
        value["frames"] = serde_json::json!([first.clone(), first.clone(), first.clone()]);
        for (index, frame) in [2, 0, 1].into_iter().enumerate() {
            value["frames"][index]["frame"] = frame.into();
        }
        let complete: Manifest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(source_frames(&complete, "fixture", 3), Some(vec![1, 2, 0]));
        assert_eq!(source_frames(&complete, "absent", 3), None);
        assert_eq!(source_frames(&complete, "fixture", 4), None);
        value["frames"][2]["frame"] = 0.into();
        let duplicate: Manifest = serde_json::from_value(value).unwrap();
        assert_eq!(source_frames(&duplicate, "fixture", 3), None);
    }

    #[wasm_bindgen_test]
    fn native_forest_source_loads_only_ten_accents_from_the_full_hundred() {
        let mut value = manifest::fixture();
        let frame = value["frames"][0].clone();
        value["frames"] = serde_json::Value::Array(vec![frame; 100]);
        let selection = OPTIONAL_TERRAIN_SOURCES[0];
        let source = selection.manifest_source();
        for index in 0..100 {
            value["frames"][index]["frame"] = (index as u32).into();
            value["frames"][index]["source"] = source.clone().into();
        }
        let manifest: Manifest = serde_json::from_value(value).unwrap();
        assert_eq!(
            source_frames(&manifest, &source, selection.frames),
            Some((0..10).collect())
        );
    }

    #[wasm_bindgen_test]
    fn startup_index_sort_matches_std_at_catalogue_bound() {
        let bound = REQUIRED_RENDER_SOURCES
            .iter()
            .chain(OPTIONAL_RESOURCE_SOURCES.iter())
            .chain(OPTIONAL_TERRAIN_SOURCES.iter())
            .map(|selection| selection.frames as usize)
            .sum::<usize>();
        for size in [0, 1, bound.saturating_sub(2), bound] {
            let mut shuffled = (0..size).collect::<Vec<_>>();
            let mut seed = 0x2a97_1845_u32;
            for index in (1..size).rev() {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                shuffled.swap(index, seed as usize % (index + 1));
            }
            for input in [
                (0..size).collect::<Vec<_>>(),
                (0..size).rev().collect(),
                vec![17; size],
                shuffled,
            ] {
                let mut actual = input.clone();
                let mut expected = input;
                expected.sort_unstable();
                let mut comparisons = 0_usize;
                sort_indices(&mut actual, &mut |left, right| {
                    comparisons += 1;
                    left.cmp(right)
                });
                assert_eq!(actual, expected);
                assert!(comparisons <= size * size.saturating_sub(1) / 2);
            }
        }
        let mut value = manifest::fixture();
        let frame = value["frames"][0].clone();
        value["frames"] = serde_json::Value::Array(vec![frame; bound + 1]);
        let oversized: Manifest = serde_json::from_value(value).unwrap();
        assert_eq!(source_frames(&oversized, "fixture", 3), None);
    }

    #[wasm_bindgen_test]
    fn index_only_packing_matches_stable_record_order_and_semantic_ties() {
        let mut value = manifest::fixture();
        let first = value["frames"][0].clone();
        value["frames"] = serde_json::Value::Array(vec![first; 5]);
        for (index, (width, height)) in [(4, 9), (8, 9), (8, 9), (20, 4), (2, 16)]
            .into_iter()
            .enumerate()
        {
            value["frames"][index]["frame"] = (index as u32).into();
            value["frames"][index]["width"] = width.into();
            value["frames"][index]["height"] = height.into();
        }
        let manifest: Manifest = serde_json::from_value(value).unwrap();
        let selected = [2, 0, 4, 1, 3];
        let mut reference = selected
            .iter()
            .enumerate()
            .map(|(index, selected)| (index, &manifest.frames[*selected]))
            .collect::<Vec<_>>();
        reference.sort_by_key(|(_, frame)| {
            (
                std::cmp::Reverse(frame.height),
                std::cmp::Reverse(frame.width),
            )
        });
        let expected = reference
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        assert_eq!(packing_order(&manifest, &selected), expected);
        assert_eq!(expected, [2, 0, 3, 1, 4]);
        assert_eq!(selected, [2, 0, 4, 1, 3]);
    }
}
