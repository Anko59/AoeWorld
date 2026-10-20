#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
#[cfg_attr(feature = "diagnostics", wasm_bindgen::prelude::wasm_bindgen)]
pub fn replay_hash(scenario: &str, ticks: u32) -> Result<String, wasm_bindgen::JsValue> {
    if ticks > 64 {
        return Err(wasm_bindgen::JsValue::from_str("ticks must be <= 64"));
    }
    let scenario = aoe_scenario::named(scenario)
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("unknown scenario"))?;
    let mut world = aoe_simulation::World::new(scenario);
    for _ in 0..ticks {
        world.advance();
    }
    Ok(world.canonical_hash_hex())
}

#[cfg(all(target_arch = "wasm32", not(feature = "diagnostics")))]
mod playground;

#[cfg(all(target_arch = "wasm32", not(feature = "diagnostics")))]
mod game_assets;

#[cfg(all(target_arch = "wasm32", not(feature = "diagnostics")))]
mod png_page;

#[cfg(all(target_arch = "wasm32", not(feature = "diagnostics")))]
mod unit_visuals;

pub mod resource_state;

#[cfg(any(all(target_arch = "wasm32", not(feature = "diagnostics")), test))]
#[path = "playground_map/requests.rs"]
mod chunk_requests;

#[cfg(all(target_arch = "wasm32", not(feature = "diagnostics")))]
#[path = "building_ui/mod.rs"]
pub mod building_ui;
