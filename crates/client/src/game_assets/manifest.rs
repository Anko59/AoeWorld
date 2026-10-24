//! Strict asset-pack shape, without retaining metadata unused by the browser.
//! Required fields and their types remain identical to the importer manifest.
#[cfg(test)]
use serde::{Deserialize, Deserializer, de::Visitor};
#[cfg(test)]
use std::fmt;

#[cfg(test)]
struct DiscardString;

#[cfg(test)]
impl<'de> Deserialize<'de> for DiscardString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;
        impl<'de> Visitor<'de> for StringVisitor {
            type Value = DiscardString;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string")
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<DiscardString, E> {
                Ok(DiscardString)
            }
        }
        deserializer.deserialize_str(StringVisitor)
    }
}

#[cfg_attr(test, derive(Deserialize))]
pub(super) struct Manifest {
    pub version: u16,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "converter"))]
    _converter: DiscardString,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "input_hash"))]
    _input_hash: DiscardString,
    pub pages: Vec<AtlasPage>,
    pub frames: Vec<FrameRecord>,
    pub building_data: String,
    #[cfg_attr(test, serde(skip))]
    pub buildings: Vec<BuildingComposition>,
}

/// Only recipe fields consumed at startup are retained. Graphic ids, terrain
/// ids and layers remain in the pack/table; the ordered recipe resolves them.
pub(super) struct BuildingComposition {
    pub unit_id: i32,
    pub footprint: [f32; 2],
    pub foundation_slp: Option<i32>,
    pub foundation_size: [u16; 2],
    pub parts: Vec<BuildingPart>,
    pub construction: Vec<BuildingPart>,
}

pub(super) struct BuildingPart {
    pub record: Option<usize>,
    pub frames: u16,
    pub angles: u16,
    pub primary: bool,
    pub frame_rate: f32,
    pub offset: [i32; 2],
    pub misplacement: [f32; 2],
}

#[cfg_attr(test, derive(Deserialize))]
pub(super) struct AtlasPage {
    pub color: String,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "color_hash"))]
    _color_hash: DiscardString,
    pub player: String,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "player_hash"))]
    _player_hash: DiscardString,
    pub shadow: String,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "shadow_hash"))]
    _shadow_hash: DiscardString,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "outline"))]
    _outline: DiscardString,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "outline_hash"))]
    _outline_hash: DiscardString,
    pub width: u16,
    pub height: u16,
}

#[cfg_attr(test, derive(Deserialize))]
pub(super) struct FrameRecord {
    pub source: String,
    #[cfg(test)]
    #[cfg_attr(test, serde(rename = "source_hash"))]
    _source_hash: DiscardString,
    pub frame: u32,
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub anchor_x: i32,
    pub anchor_y: i32,
}

#[cfg(test)]
#[path = "tests/manifest.rs"]
mod tests;
#[cfg(test)]
pub(super) use tests::fixture;

use js_sys::Array;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(module = "/src/game_assets/manifest.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn parse_manifest(bytes: &[u8]) -> Result<Array, JsValue>;
    #[wasm_bindgen(catch, js_name = parse_chunk)]
    fn chunk_json(text: &str) -> Result<Array, JsValue>;
    #[wasm_bindgen(catch, js_name = parse_chunks)]
    fn chunks_json(text: &str) -> Result<Array, JsValue>;
    #[wasm_bindgen(catch, js_name = parse_bounds)]
    fn bounds_json(text: &str, hash: &str) -> Result<Array, JsValue>;
}

fn text(fields: &Array, index: u32) -> Result<String, JsValue> {
    fields
        .get(index)
        .as_string()
        .ok_or_else(|| JsValue::from_str("invalid manifest string projection"))
}
fn number(fields: &Array, index: u32) -> Result<f64, JsValue> {
    fields
        .get(index)
        .as_f64()
        .ok_or_else(|| JsValue::from_str("invalid manifest number projection"))
}

pub(super) fn decode(bytes: &[u8]) -> Result<Manifest, JsValue> {
    let root = parse_manifest(bytes)?;
    let pages = Array::from(&root.get(2))
        .iter()
        .map(|value| {
            let fields = Array::from(&value);
            Ok(AtlasPage {
                color: text(&fields, 0)?,
                player: text(&fields, 1)?,
                shadow: text(&fields, 2)?,
                width: number(&fields, 3)? as u16,
                height: number(&fields, 4)? as u16,
                #[cfg(test)]
                _color_hash: DiscardString,
                #[cfg(test)]
                _player_hash: DiscardString,
                #[cfg(test)]
                _shadow_hash: DiscardString,
                #[cfg(test)]
                _outline: DiscardString,
                #[cfg(test)]
                _outline_hash: DiscardString,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    let frames = Array::from(&root.get(3))
        .iter()
        .map(|value| {
            let fields = Array::from(&value);
            Ok(FrameRecord {
                source: text(&fields, 0)?,
                frame: number(&fields, 1)? as u32,
                page: number(&fields, 2)? as u16,
                x: number(&fields, 3)? as u16,
                y: number(&fields, 4)? as u16,
                width: number(&fields, 5)? as u16,
                height: number(&fields, 6)? as u16,
                anchor_x: number(&fields, 7)? as i32,
                anchor_y: number(&fields, 8)? as i32,
                #[cfg(test)]
                _source_hash: DiscardString,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(Manifest {
        version: number(&root, 0)? as u16,
        building_data: text(&root, 1)?,
        pages,
        frames,
        buildings: Vec::new(),
        #[cfg(test)]
        _converter: DiscardString,
        #[cfg(test)]
        _input_hash: DiscardString,
    })
}

fn compact(fields: Array) -> Result<aoe_map::CompactChunk, JsValue> {
    Ok(aoe_map::CompactChunk {
        x: number(&fields, 0)? as i32,
        y: number(&fields, 1)? as i32,
        payload_hex: text(&fields, 2)?,
    })
}
pub(crate) fn decode_chunk(text: &str) -> Result<aoe_map::CompactChunk, JsValue> {
    compact(chunk_json(text)?)
}
pub(crate) fn decode_chunks(text: &str) -> Result<Vec<aoe_map::CompactChunk>, JsValue> {
    chunks_json(text)?
        .iter()
        .map(|value| compact(Array::from(&value)))
        .collect()
}
pub(crate) fn decode_bounds(text: &str, hash: &str) -> Result<(i16, i16), JsValue> {
    let bounds = bounds_json(text, hash)?;
    Ok((number(&bounds, 0)? as i16, number(&bounds, 1)? as i16))
}
