#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
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

#[cfg(target_arch = "wasm32")]
mod playground;

#[cfg(target_arch = "wasm32")]
mod game_assets;

pub mod resource_state;
