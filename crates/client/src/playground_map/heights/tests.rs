use super::parse_bounds;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn compact_bounds_decoder_keeps_identity_integer_range_and_order_checks() {
    let bounds = |hash: &str, min: &str, max: &str| {
        format!(
            r#"{{"content_hash":"{hash}","minimum_height_level":{min},"maximum_height_level":{max}}}"#
        )
    };
    assert_eq!(
        parse_bounds(&bounds("map", "-32768", "32767"), "map").unwrap(),
        (-32768, 32767)
    );
    for (hash, min, max) in [
        ("other", "0", "1"),
        ("map", "2", "1"),
        ("map", "-32769", "0"),
        ("map", "0", "32768"),
        ("map", "0.5", "1"),
        ("map", "null", "1"),
        ("map", "\"0\"", "1"),
    ] {
        assert!(parse_bounds(&bounds(hash, min, max), "map").is_err());
    }
    assert!(parse_bounds(r#"{"content_hash":"map"}"#, "map").is_err());
    assert!(parse_bounds(r#"["map",0,1]"#, "map").is_err());
}
