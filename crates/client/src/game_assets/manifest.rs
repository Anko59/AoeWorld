//! Strict asset-pack shape, without retaining metadata unused by the browser.
//! Required fields and their types remain identical to the importer manifest.
use serde::{Deserialize, Deserializer, de::Visitor};
use std::fmt;

struct DiscardString;

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

#[derive(Deserialize)]
pub(super) struct Manifest {
    pub version: u16,
    #[serde(rename = "converter")]
    _converter: DiscardString,
    #[serde(rename = "input_hash")]
    _input_hash: DiscardString,
    pub pages: Vec<AtlasPage>,
    pub frames: Vec<FrameRecord>,
    pub building_data: String,
    #[serde(skip)]
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

#[derive(Deserialize)]
pub(super) struct AtlasPage {
    pub color: String,
    #[serde(rename = "color_hash")]
    _color_hash: DiscardString,
    pub player: String,
    #[serde(rename = "player_hash")]
    _player_hash: DiscardString,
    pub shadow: String,
    #[serde(rename = "shadow_hash")]
    _shadow_hash: DiscardString,
    #[serde(rename = "outline")]
    _outline: DiscardString,
    #[serde(rename = "outline_hash")]
    _outline_hash: DiscardString,
    pub width: u16,
    pub height: u16,
}

#[derive(Deserialize)]
pub(super) struct FrameRecord {
    pub source: String,
    #[serde(rename = "source_hash")]
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
