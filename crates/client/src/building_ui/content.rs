//! Startup metadata stays in the generated JSON asset, outside production WASM.
use aoe_assets::catalog::units::ActorSource;
use aoe_simulation::BrowserContent;
use js_sys::{Array, Int32Array, Uint32Array};
use std::sync::OnceLock;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

struct Content {
    rules: BrowserContent,
    actors: Vec<ActorSource>,
}
static CONTENT: OnceLock<Content> = OnceLock::new();

#[wasm_bindgen(module = "/src/game_assets/manifest.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn parse_content(bytes: &[u8], actor_count: usize) -> Result<Array, JsValue>;
}

fn get() -> &'static Content {
    #[cfg(test)]
    CONTENT.get_or_init(|| Content {
        rules: BrowserContent::decode(&aoe_simulation::browser_content()).unwrap(),
        actors: aoe_assets::catalog::units::SPRITES.to_vec(),
    });
    CONTENT
        .get()
        .expect("game content must load before the scene")
}

pub(crate) fn rules() -> &'static BrowserContent {
    &get().rules
}
pub(crate) fn sprites() -> &'static [ActorSource] {
    &get().actors
}

pub(crate) async fn load() -> Result<(), JsValue> {
    if CONTENT.get().is_some() {
        return Ok(());
    }
    let bytes = crate::game_assets::fetch("/pkg/content.json").await?;
    let _ = CONTENT.set(decode(&bytes)?);
    Ok(())
}

fn decode(bytes: &[u8]) -> Result<Content, JsValue> {
    let fields = parse_content(bytes, aoe_assets::catalog::units::SPRITES.len())?;
    let rules = BrowserContent::decode(&Int32Array::new(&fields.get(0)).to_vec())
        .map_err(JsValue::from_str)?;
    let words = Uint32Array::new(&fields.get(1)).to_vec();
    let actors: Vec<ActorSource> = words
        .as_chunks::<4>()
        .0
        .iter()
        .map(|r| ActorSource {
            id: r[0],
            stride: r[1] as u8,
            millis: r[2] as u16,
            phases: r[3] as u8,
        })
        .collect();
    for (source, expected) in actors.iter().zip(aoe_assets::catalog::units::SPRITES) {
        if (source.id, source.stride, source.millis, source.phases)
            != (
                expected.id,
                expected.stride,
                expected.millis,
                expected.phases,
            )
        {
            return Err(JsValue::from_str("Unsupported native actor content"));
        }
    }
    Ok(Content { rules, actors })
}

#[cfg(test)]
mod tests;
