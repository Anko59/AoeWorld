//! Unwired driver qualification: actual device and the same production render pass.
use super::*;
use crate::surface_mesh::world::fixtures as fixture;
use wasm_bindgen::{JsCast, JsValue, prelude::*};
use wasm_bindgen_test::*;

#[wasm_bindgen(module = "/src/web/bridge.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = createWebGpu)]
    async fn create(
        canvas: &HtmlCanvasElement,
        shader: &str,
        capacity: u32,
    ) -> Result<Bridge, JsValue>;
    type Bridge;
    #[wasm_bindgen(method, catch, js_name = uploadAtlas)]
    fn upload(this: &Bridge, bytes: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = writePreparedInstances)]
    fn write(this: &Bridge, bytes: &[u8]) -> Result<u32, JsValue>;
    #[wasm_bindgen(method, catch, js_name = renderPreparedInstances)]
    fn render(this: &Bridge, bytes: &[u8], clear: &[f64]) -> Result<bool, JsValue>;
    #[wasm_bindgen(method, catch, js_name = createTestReadback)]
    fn reader(this: &Bridge, count: u32) -> Result<Readback, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn diagnostics(this: &Bridge) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(method)]
    fn dispose(this: &Bridge);
    type Readback;
    #[wasm_bindgen(method, catch)]
    async fn read(
        this: &Readback,
        count: u32,
        points: &JsValue,
        clear: &[f64],
    ) -> Result<js_sys::Uint8Array, JsValue>;
    #[wasm_bindgen(method, js_name = dispose)]
    fn dispose_reader(this: &Readback);
}

fn canvas() -> HtmlCanvasElement {
    let canvas = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    canvas
}
fn property(value: &JsValue, name: &str) -> f64 {
    js_sys::Reflect::get(value, &JsValue::from_str(name))
        .unwrap()
        .as_f64()
        .unwrap()
}
async fn pixels(
    bridge: &Bridge,
    reader: &Readback,
    faces: &[ProjectedSurfaceTriangle],
    points: [[u32; 2]; 5],
) -> [[u8; 4]; 5] {
    let mut packets = faces
        .iter()
        .map(|face| surface_instance(face, [128.0, 128.0], 0.0))
        .collect::<Vec<_>>();
    normalize_depths(&mut packets);
    let count = bridge.write(bytemuck::cast_slice(&packets)).unwrap();
    let probes = js_sys::Array::new();
    for point in points {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from(point[0]));
        pair.push(&JsValue::from(point[1]));
        probes.push(&pair);
    }
    let bytes = reader
        .read(count, &probes, &[0.0, 0.0, 0.0, 1.0])
        .await
        .unwrap()
        .to_vec();
    assert_eq!(bytes.len(), 20);
    std::array::from_fn(|index| bytes[index * 4..index * 4 + 4].try_into().unwrap())
}

#[wasm_bindgen_test]
async fn prototype_same_production_pass_world_native_lod_and_fault_fallback_pixels() {
    let canvas = canvas();
    let bridge = create(&canvas, SHADER_SOURCE, CAPACITY as u32)
        .await
        .unwrap();
    let before = bridge.diagnostics().unwrap();
    assert_eq!(property(&before, "gpuBufferBytes"), (CAPACITY * 112) as f64);
    assert_eq!(property(&before, "atlasBytes"), 256.0);
    assert_eq!(property(&before, "persistentGpuResources"), 7.0);
    assert!(bridge.upload(&[]).is_err());
    assert!(bridge.reader(0).is_err());
    assert!(bridge.write(&[0; 113]).is_err());
    let mut data = fixture::Fixture::new();
    bridge.upload(&data.atlas).unwrap();
    let after = bridge.diagnostics().unwrap();
    assert_eq!(
        property(&after, "atlasBytes"),
        crate::GAME_ATLAS_BYTES as f64
    );
    assert_eq!(property(&after, "persistentGpuResources"), 7.0);
    let reader = bridge.reader(5).unwrap();
    for case in fixture::cases() {
        let expected = fixture::probes(case).map(|point| fixture::expected(case, point));
        let coarse = pixels(
            &bridge,
            &reader,
            &fixture::faces(&data.art, case, false),
            fixture::probes(case),
        )
        .await;
        let fine = pixels(
            &bridge,
            &reader,
            &fixture::faces(&data.art, case, true),
            fixture::probes(case),
        )
        .await;
        for index in 0..5 {
            fixture::assert_rgba(coarse[index], expected[index]);
            fixture::assert_rgba(fine[index], expected[index]);
            fixture::assert_rgba(fine[index], coarse[index]);
        }
    }
    let case = fixture::cases()[4];
    let cached = fixture::faces(&data.art, case, false);
    let legacy = fixture::legacy(&cached);
    for zero in [false, true] {
        data.corrupt(zero);
        bridge.upload(&data.atlas).unwrap();
        let guarded = pixels(&bridge, &reader, &cached, fixture::probes(case)).await;
        let old = pixels(&bridge, &reader, &legacy, fixture::probes(case)).await;
        assert_eq!(guarded, old);
    }
    assert!(bridge.render(&[], &[0.0, 0.0, 0.0, 1.0]).unwrap());
    reader.dispose_reader();
    assert!(
        reader
            .read(0, &js_sys::Array::new(), &[0.0, 0.0, 0.0, 1.0])
            .await
            .is_err()
    );
    bridge.dispose();
    assert!(bridge.diagnostics().is_err());
}
