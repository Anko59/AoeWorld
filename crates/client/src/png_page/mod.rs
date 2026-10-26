//! Bounded, byte-exact RGBA8 PNG decoding lives in the browser runtime asset.
use js_sys::{Promise, Uint8Array};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(module = "/src/png_page/decoder.js")]
extern "C" {
    fn decode_page(bytes: &[u8]) -> Promise;
}
pub(super) async fn decode(bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    // JavaScript validates and owns compressed data synchronously, before yielding.
    let pending = decode_page(&bytes);
    drop(bytes);
    let value = JsFuture::from(pending).await?;
    Ok(Uint8Array::new(&value).to_vec())
}

#[cfg(test)]
mod support_tests;
#[cfg(test)]
use support_tests::*;
#[cfg(test)]
mod tests;
