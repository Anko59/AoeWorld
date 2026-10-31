use super::*;
use aoe_assets::pack::{
    compressed,
    frame_chunks::{FrameChunk, FrameLocation},
};
use wasm_bindgen_test::wasm_bindgen_test;
#[path = "tests/runtime.rs"]
mod runtime;

#[wasm_bindgen_test]
fn native_selection_shares_payloads_without_merging_semantic_geometry_or_mask_modes() {
    let mut value = manifest::fixture();
    value["chunks"] =
        serde_json::json!([{"file":"frames-0000.bin","hash":"a".repeat(64),"bytes":64}]);
    value["frames"][0]["native"] = serde_json::json!({"chunk":0,"offset":0,"length":64});
    let mut second = value["frames"][0].clone();
    second["frame"] = 1.into();
    second["anchor_x"] = 80.into();
    value["frames"].as_array_mut().unwrap().push(second);
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let mut records = manifest
        .frames
        .iter()
        .map(|frame| placement::record(frame, Placement::default()))
        .collect::<Vec<_>>();
    let native = prepare(
        &manifest,
        &[0, 1],
        &[true, true],
        &[],
        [65, 145, 245],
        &[Placement::default(); 2],
        &mut records,
    )
    .unwrap()
    .unwrap();
    assert_eq!(native.len(), 1);
    assert_eq!((records[0].stream, records[1].stream), (1, 1));
    assert_eq!(records[0].anchor, [1.0, 2.0]);
    assert_eq!(records[1].anchor, [80.0, 2.0]);
    let native = prepare(
        &manifest,
        &[0, 1],
        &[true, false],
        &[],
        [65, 145, 245],
        &[Placement::default(); 2],
        &mut records,
    )
    .unwrap()
    .unwrap();
    assert_eq!(native.len(), 2);
    assert_ne!(records[0].stream, records[1].stream);
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "
let native_original_fetch;
export function native_response(bytes,declared) {
 native_original_fetch=window.fetch;
 window.fetch=async()=>new Response(bytes.slice(),{status:200,headers:{'Content-Length':String(declared)}});
}
export function native_restore_fetch(){window.fetch=native_original_fetch;}
")]
extern "C" {
    fn native_response(bytes: &[u8], declared: u32);
    fn native_restore_fetch();
}

#[wasm_bindgen_test]
async fn native_fetch_enforces_declared_and_streamed_bytes_before_acceptance() {
    for (bytes, declared, expected, valid) in [
        (vec![1, 2, 3], 3, 3, true),
        (vec![1, 2, 3, 4], 3, 3, false),
        (vec![1, 2], 3, 3, false),
        (vec![1, 2, 3], 4, 3, false),
    ] {
        native_response(&bytes, declared);
        let result = fetch_chunk("frames-0000.bin", expected).await;
        native_restore_fetch();
        assert_eq!(result.is_ok(), valid);
        if valid {
            assert_eq!(js_sys::Uint8Array::new(&result.unwrap()).to_vec(), bytes);
        }
    }
}

fn cell(chunk: u16, length: u32) -> Cell {
    Cell {
        location: FrameLocation {
            chunk,
            offset: 0,
            length,
        },
        size: [1, 1],
        owned: true,
        team: [65, 145, 245],
    }
}

#[wasm_bindgen_test]
fn native_requests_share_chunks_and_bound_concurrent_bytes_before_fetching() {
    let chunks: Vec<_> = (0..16)
        .map(|i| FrameChunk {
            file: format!("frames-{i:04}.bin"),
            hash: "a".repeat(64),
            bytes: 20 * 1024 * 1024,
        })
        .collect();
    let cells = (0..16).map(|i| cell(i, 64)).collect();
    let mut native = NativeFrames::new(chunks, cells, 8).unwrap();
    assert_eq!(native.layer_count(), 16);
    assert_eq!(native.requests(&[1, 1, 2, 3, 4, 5]).unwrap(), vec![0, 1, 2]);
    assert!(native.requests(&[1, 2, 3, 4, 5]).unwrap().is_empty());
    assert!(native.requests(&[17]).is_err());
}

#[wasm_bindgen_test]
fn wanted_chunks_larger_than_the_encoded_budget_still_make_forward_progress() {
    let bytes = 20 * 1024 * 1024;
    let chunks = (0..8)
        .map(|i| FrameChunk {
            file: format!("frames-{i:04}.bin"),
            hash: "a".repeat(64),
            bytes: bytes as u32,
        })
        .collect();
    let cells = (0..8).map(|i| cell(i, 64)).collect();
    let mut native = NativeFrames::new(chunks, cells, 8).unwrap();
    // Request policy only: payload validation/decode is tested separately.
    for i in 0..4 {
        native.encoded[i] = Some(Encoded {
            bytes: vec![0; bytes],
            touched: i as u64 + 1,
        });
    }
    native.clock = 4;
    let requests = native.requests(&(1..=8).collect::<Vec<_>>()).unwrap();
    assert_eq!(
        requests,
        [4, 5, 6],
        "wanted data must not pin the encoded budget forever"
    );
    assert!(native.cached_bytes() + native.inflight_bytes() <= ENCODED_BYTES);
    assert!(native.inflight_bytes() <= INFLIGHT_BYTES);
    assert!(native.inflight.len() <= MAX_REQUESTS);
    assert!(
        native.encoded[3].is_some(),
        "retain the most recently used chunk"
    );
}

#[wasm_bindgen_test]
fn native_chunks_verify_payload_and_dimensions_before_the_frame_can_be_uploaded() {
    let planes = [
        vec![117, 120, 115, 128],
        vec![7, 0, 0, 119],
        vec![0, 0, 0, 77],
    ];
    let bytes = compressed::encode_frame([1, 1], planes.each_ref().map(Vec::as_slice)).unwrap();
    let chunk = FrameChunk::from_bytes("frames-0000.bin".into(), &bytes).unwrap();
    let mut native = NativeFrames::new(vec![chunk], vec![cell(0, bytes.len() as u32)], 8).unwrap();
    assert!(native.decode(1).unwrap().is_none());
    assert_eq!(native.requests(&[1]).unwrap(), vec![0]);
    let mut tampered = bytes.clone();
    tampered[0] ^= 1;
    assert!(native.accept(0, tampered).is_err());
    native.accept(0, bytes).unwrap();
    assert_eq!(native.decode(1).unwrap().unwrap(), planes);
    native.cells[0].size = [2, 1];
    assert!(native.decode(1).is_err());
}

#[wasm_bindgen_test]
fn native_metadata_rejects_paths_invalid_ranges_and_unbounded_layer_budgets() {
    let chunk = FrameChunk {
        file: "frames-0000.bin".into(),
        hash: "a".repeat(64),
        bytes: 64,
    };
    assert!(NativeFrames::new(vec![chunk.clone()], vec![cell(0, 65)], 8).is_err());
    assert!(NativeFrames::new(vec![chunk.clone()], vec![cell(1, 64)], 8).is_err());
    assert!(NativeFrames::new(vec![chunk.clone()], vec![cell(0, 64)], 64).is_err());
    for name in [
        "../escape.bin",
        "buildings/../escape.bin",
        "/escape.bin",
        "x\\y.bin",
        ".",
        "..",
    ] {
        let mut invalid = chunk.clone();
        invalid.file = name.into();
        assert!(
            NativeFrames::new(vec![invalid], vec![cell(0, 64)], 8).is_err(),
            "{name}"
        );
    }
}

#[wasm_bindgen_test]
fn native_upload_tiles_preserve_owner_weights_shadow_and_transparent_gutters() {
    let planes = [
        vec![117, 120, 115, 128, 0, 0, 0, 0],
        vec![7, 0, 0, 119, 0, 0, 0, 0],
        vec![0, 0, 0, 77, 0, 0, 0, 88],
    ];
    let placement = Placement {
        page: 8,
        x: 2,
        y: 2,
        width: 2,
        height: 1,
    };
    let [colour, mask] = pixels::compose(&planes, placement, true, [65, 145, 245]).unwrap();
    let at = (2 * 64 + 2) * 4;
    assert_eq!(colour.len(), 64 * 64 * 4);
    assert_eq!(&colour[at..at + 4], &[117, 120, 115, 187]);
    assert_eq!(&mask[at..at + 4], &[7, 119, 128, 0]);
    assert_eq!(&colour[at + 4..at + 8], &[0, 0, 0, 88]);
    assert_eq!(&colour[..4], &[255; 4]);
    assert_eq!(&mask[..4], &[0; 4]);
    assert_eq!(&colour[4..8], &[0; 4]);
    let [colour, mask] = pixels::compose(&planes, placement, false, [65, 145, 245]).unwrap();
    assert_eq!(&colour[at..at + 4], &[84, 136, 198, 187]);
    assert!(mask.iter().all(|b| *b == 0));
    assert!(
        pixels::compose(
            &[Vec::new(), Vec::new(), Vec::new()],
            placement,
            true,
            [0; 3]
        )
        .is_err()
    );
}
