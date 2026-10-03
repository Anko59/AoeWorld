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
    #[serde(rename = "width")]
    _width: u16,
    #[serde(rename = "height")]
    _height: u16,
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
pub(super) fn fixture() -> serde_json::Value {
    serde_json::json!({
        "version": 1, "converter": "test\nconverter", "input_hash": "hash",
        "pages": [{
            "color": "color.png", "color_hash": "hash",
            "player": "player.png", "player_hash": "hash",
            "shadow": "shadow.png", "shadow_hash": "hash",
            "outline": "outline.png", "outline_hash": "hash",
            "width": 2048, "height": 2048
        }],
        "frames": [{
            "source": "fixture", "source_hash": "hash", "frame": 0,
            "page": 0, "x": 0, "y": 0, "width": 2, "height": 4,
            "anchor_x": 1, "anchor_y": 2
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_assets::pack::Manifest as OriginalManifest;
    use serde_json::{Map, Value};
    use wasm_bindgen_test::wasm_bindgen_test;

    fn object<'a>(value: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
        match section {
            "root" => value.as_object_mut().unwrap(),
            "page" => value["pages"][0].as_object_mut().unwrap(),
            "frame" => value["frames"][0].as_object_mut().unwrap(),
            _ => unreachable!(),
        }
    }

    fn reject_like_original(bytes: &[u8]) {
        let optimized = serde_json::from_slice::<Manifest>(bytes).err().unwrap();
        let original = serde_json::from_slice::<OriginalManifest>(bytes)
            .err()
            .unwrap();
        assert_eq!(optimized.to_string(), original.to_string());
    }

    #[wasm_bindgen_test]
    fn unused_metadata_still_requires_strings_and_rejects_duplicate_fields() {
        for (section, field) in [
            ("root", "converter"),
            ("root", "input_hash"),
            ("page", "color_hash"),
            ("page", "player_hash"),
            ("page", "shadow_hash"),
            ("page", "outline"),
            ("page", "outline_hash"),
            ("frame", "source_hash"),
        ] {
            let mut missing = fixture();
            object(&mut missing, section).remove(field);
            reject_like_original(&serde_json::to_vec(&missing).unwrap());
            for invalid in [Value::Null, serde_json::json!(5), serde_json::json!([])] {
                let mut invalid_value = fixture();
                object(&mut invalid_value, section).insert(field.into(), invalid);
                reject_like_original(&serde_json::to_vec(&invalid_value).unwrap());
            }
            let mut duplicate = fixture();
            object(&mut duplicate, section).insert(field.into(), "hash".into());
            let raw = serde_json::to_string(&duplicate).unwrap();
            let repeated = raw.replace(
                &format!("\"{field}\":\"hash\""),
                &format!("\"{field}\":\"first\",\"{field}\":\"second\""),
            );
            reject_like_original(repeated.as_bytes());
        }
    }

    #[wasm_bindgen_test]
    fn used_fields_and_page_dimension_guards_preserve_the_original_shape() {
        let bytes = serde_json::to_vec(&fixture()).unwrap();
        let optimized: Manifest = serde_json::from_slice(&bytes).unwrap();
        let original: OriginalManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(optimized.version, original.version);
        assert_eq!(optimized.pages[0].color, original.pages[0].color);
        assert_eq!(optimized.pages[0].player, original.pages[0].player);
        assert_eq!(optimized.pages[0].shadow, original.pages[0].shadow);
        assert_eq!(optimized.frames[0].source, original.frames[0].source);
        assert_eq!(optimized.frames[0].anchor_x, original.frames[0].anchor_x);
        for field in ["width", "height"] {
            for invalid in [
                Value::Null,
                serde_json::json!(-1),
                serde_json::json!(65_536),
                serde_json::json!("2048"),
            ] {
                let mut value = fixture();
                object(&mut value, "page").insert(field.into(), invalid);
                reject_like_original(&serde_json::to_vec(&value).unwrap());
            }
            let mut value = fixture();
            object(&mut value, "page").remove(field);
            reject_like_original(&serde_json::to_vec(&value).unwrap());
        }
        assert_eq!(std::mem::size_of::<DiscardString>(), 0);
        assert!(
            std::mem::size_of::<FrameRecord>()
                < std::mem::size_of::<aoe_assets::pack::FrameRecord>()
        );
        assert!(
            std::mem::size_of::<AtlasPage>() < std::mem::size_of::<aoe_assets::pack::AtlasPage>()
        );
    }
}
