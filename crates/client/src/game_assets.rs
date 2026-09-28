//! Loads only the local pack pages needed by the first map and compacts them.
use aoe_assets::catalog::{AssetRole, OPTIONAL_RESOURCE_SOURCES, REQUIRED_RENDER_SOURCES};
use aoe_assets::pack::{FrameRecord, Manifest};
use aoe_rendering::{GAME_ATLAS_SIDE, GameArt, GameFrame};
use js_sys::Uint8Array;
use std::collections::BTreeMap;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::Response;

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

pub async fn load() -> Result<(GameArt, Vec<u8>), JsValue> {
    let bytes = fetch("/asset-pack/manifest.json").await?;
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(error)?;
    if manifest.version != 1 {
        return Err(JsValue::from_str("Unsupported asset pack version"));
    }
    let mut selected = Vec::new();
    let mut ranges = BTreeMap::new();
    for (selection, required) in REQUIRED_RENDER_SOURCES
        .into_iter()
        .map(|selection| (selection, true))
        .chain(
            OPTIONAL_RESOURCE_SOURCES
                .into_iter()
                .map(|selection| (selection, false)),
        )
    {
        let source = selection.manifest_source();
        let mut frames: Vec<_> = manifest
            .frames
            .iter()
            .filter(|f| f.source == source && f.frame < selection.frames)
            .cloned()
            .collect();
        frames.sort_by_key(|f| f.frame);
        let complete = frames.len() == selection.frames as usize
            && frames.iter().enumerate().all(|(i, f)| f.frame == i as u32);
        if !complete && required {
            return Err(JsValue::from_str(&format!(
                "Local pack is missing reviewed {:?} art ({})",
                selection.role, selection.id
            )));
        }
        if !complete {
            continue;
        }
        let start = selected.len();
        selected.extend(frames);
        ranges.insert(selection.role, start..selected.len());
    }
    let side = GAME_ATLAS_SIDE as usize;
    let mut pixels = vec![0; side * side * 4];
    pixels[..4].copy_from_slice(&[255; 4]);
    let mut records = Vec::new();
    let mut placements = BTreeMap::<u16, Vec<(FrameRecord, usize, usize)>>::new();
    let (mut x, mut y, mut row_height) = (2, 2, 0);
    for f in selected {
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
        records.push(GameFrame {
            uv: [
                x as f32 / side as f32,
                y as f32 / side as f32,
                w as f32 / side as f32,
                h as f32 / side as f32,
            ],
            size: [w as f32, h as f32],
            anchor: [f.anchor_x as f32, f.anchor_y as f32],
        });
        placements.entry(f.page).or_default().push((f, x, y));
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
        for (frame, x, y) in frames {
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
    let group = |role| {
        ranges
            .get(&role)
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
        },
        pixels,
    ))
}
