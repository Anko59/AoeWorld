use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn fixture(id: u32, count: u32) -> Manifest {
    let mut json = manifest::fixture();
    let base = json["frames"][0].clone();
    json["frames"] = serde_json::Value::Array(
        (0..count)
            .map(|frame| {
                let mut value = base.clone();
                value["source"] = format!("graphics.drs:[32, 112, 108, 115]:{id}").into();
                value["frame"] = frame.into();
                value
            })
            .collect(),
    );
    serde_json::from_value(json).unwrap()
}

#[wasm_bindgen_test]
fn fishing_sources_preserve_native_rows_and_reject_present_truncation() {
    assert!(
        fishing::select(&fixture(0, 1), &mut Vec::new(), &mut Vec::new())
            .unwrap()
            .iter()
            .all(Option::is_none),
        "unused slot is not sprite source zero"
    );
    let manifest = fixture(3575, 140);
    let mut selected = Vec::new();
    let ranges = fishing::select(&manifest, &mut selected, &mut Vec::new()).unwrap();
    assert_eq!(ranges[10], Some(0..35));
    assert_eq!(&selected[..7], &[0, 4, 8, 12, 16, 20, 24]);
    assert_eq!(selected[7], 28, "next native facing row");
    assert!(fishing::select(&fixture(444, 4), &mut Vec::new(), &mut Vec::new()).is_err());
    let mut selected = Vec::new();
    let ranges = fishing::select(&fixture(1911, 49), &mut selected, &mut Vec::new()).unwrap();
    assert_eq!(ranges[2], Some(0..7));
    assert_eq!(selected, [0, 7, 14, 21, 28, 35, 42]);
}
