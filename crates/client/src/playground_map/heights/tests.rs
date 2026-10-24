use super::parse_bounds;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn height_probe_fifo_reuses_entries_and_evicts_old_coordinates_on_repeated_sweeps() {
    let mut probes = std::collections::BTreeMap::new();
    let mut order = std::collections::VecDeque::new();
    for _ in 0..3 {
        for x in 0..24 {
            super::remember_probe(&mut probes, &mut order, (x, 0), (0, 1), 8);
            super::remember_probe(&mut probes, &mut order, (x, 0), (2, 3), 8);
            assert!(probes.len() <= 8);
            assert_eq!(order.len(), probes.len());
            assert_eq!(probes[&(x, 0)], (2, 3));
        }
        assert_eq!(
            probes.keys().copied().collect::<Vec<_>>(),
            (16..24).map(|x| (x, 0)).collect::<Vec<_>>()
        );
    }
}

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
