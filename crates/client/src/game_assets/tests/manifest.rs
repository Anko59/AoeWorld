pub(crate) fn fixture() -> serde_json::Value {
    serde_json::json!({
        "version": 1, "converter": "test\nconverter", "input_hash": "hash", "buildings": [], "building_data":"00",
        "chunks": [], "pages": [{
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

use super::*;
use aoe_assets::pack::Manifest as OriginalManifest;
use serde_json::{Map, Value};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn native_manifest_projection_preserves_chunk_ranges_and_rejects_invalid_shapes() {
    let mut value = fixture();
    value["chunks"] =
        serde_json::json!([{"file":"frames-0000.bin","hash":"a".repeat(64),"bytes":88}]);
    value["frames"][0]["native"] = serde_json::json!({"chunk":0,"offset":3,"length":85});
    let projected = super::super::decode_manifest(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(projected.chunks[0].bytes, 88);
    assert_eq!(projected.chunks[0].file, "frames-0000.bin");
    assert_eq!(projected.frames[0].native.unwrap().offset, 3);
    for (pointer, replacement) in [
        ("/chunks/0/bytes", serde_json::json!(-1)),
        ("/chunks/0/hash", serde_json::json!(3)),
        ("/chunks/0/file", serde_json::Value::Null),
        ("/frames/0/native/chunk", serde_json::json!(65536)),
        ("/frames/0/native/offset", serde_json::json!(4294967296_u64)),
        ("/frames/0/native/length", serde_json::json!(0.5)),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            super::super::decode_manifest(&serde_json::to_vec(&invalid).unwrap()).is_err(),
            "{pointer}"
        );
    }
}

fn object<'a>(value: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    match section {
        "root" => value.as_object_mut().unwrap(),
        "page" => value["pages"][0].as_object_mut().unwrap(),
        "frame" => value["frames"][0].as_object_mut().unwrap(),
        _ => unreachable!(),
    }
}

fn reject_like_original(bytes: &[u8]) {
    assert!(
        super::super::decode_manifest(bytes).is_err(),
        "startup decoder accepted invalid metadata"
    );
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
        std::mem::size_of::<FrameRecord>() < std::mem::size_of::<aoe_assets::pack::FrameRecord>()
    );
    assert!(std::mem::size_of::<AtlasPage>() < std::mem::size_of::<aoe_assets::pack::AtlasPage>());
}

#[wasm_bindgen_test]
fn startup_manifest_projection_preserves_current_fields_and_integer_bounds() {
    let bytes = serde_json::to_vec(&fixture()).unwrap();
    let projected = super::super::decode_manifest(&bytes).unwrap();
    assert_eq!(projected.frames[0].anchor_y, 2);
    assert_eq!(projected.pages[0].width, 2048);
    assert_eq!(projected.building_data, "00");
    for (section, field, too_large) in [
        ("root", "version", 65536_i64),
        ("frame", "frame", 4294967296),
        ("frame", "anchor_x", 2147483648),
        ("frame", "x", 65536),
    ] {
        for invalid in [
            Value::Null,
            serde_json::json!(too_large),
            serde_json::json!("1"),
            serde_json::json!(1.5),
        ] {
            let mut value = fixture();
            object(&mut value, section).insert(field.into(), invalid);
            assert!(super::super::decode_manifest(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
}

#[wasm_bindgen_test]
fn browser_chunk_projection_rejects_wrong_types_ranges_and_duplicate_fields() {
    let chunk = super::decode_chunk(r#"{"x":-2,"y":7,"payload_hex":"abcd"}"#).unwrap();
    assert_eq!(
        (chunk.x, chunk.y, chunk.payload_hex.as_str()),
        (-2, 7, "abcd")
    );
    for invalid in [
        r#"{"x":0,"x":1,"y":2,"payload_hex":"00"}"#,
        r#"{"x":2147483648,"y":0,"payload_hex":"00"}"#,
        r#"{"x":0.5,"y":0,"payload_hex":"00"}"#,
        r#"{"x":0,"y":0,"payload_hex":null}"#,
        r#"[0,0,"00"]"#,
    ] {
        assert!(super::decode_chunk(invalid).is_err());
    }
}
