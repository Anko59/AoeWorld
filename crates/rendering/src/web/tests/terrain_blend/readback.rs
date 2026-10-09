//! Bounded actual-driver readback resources, never shared across renderers.
use super::*;
use crate::web::gpu_bridge::TestReadback;
use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

pub(super) struct PixelReadback<const N: usize> {
    reader: TestReadback,
    format: String,
}

impl<const N: usize> PixelReadback<N> {
    pub(super) fn new(renderer: &Renderer) -> Self {
        assert!((1..=5).contains(&N), "bounded GPU readback probe count");
        // The driver allocates the same 128² color target and N*256-byte staging
        // buffer, retaining a view of this renderer's existing depth attachment.
        Self {
            reader: renderer
                .device
                .create_readback(N as u32)
                .expect("GPU readback resources"),
            format: renderer.config.format.clone(),
        }
    }

    // &mut self prevents overlapping maps. The common production pass makes all
    // original probe copies before mapping, resolves failures, and finally unmaps.
    pub(super) async fn read(
        &mut self,
        renderer: &Renderer,
        count: u32,
        points: [[u32; 2]; N],
        clear: [f64; 4],
    ) -> [[u8; 4]; N] {
        assert!((1..=5).contains(&N), "bounded GPU readback probe count");
        assert!(points.iter().all(|p| p[0] < 128 && p[1] < 128));
        assert_eq!(renderer.config.format, self.format);
        let probes = Array::new();
        for [x, y] in points {
            let point = Array::new();
            point.push(&JsValue::from(x));
            point.push(&JsValue::from(y));
            probes.push(&point);
        }
        let promise = self
            .reader
            .read(count, &probes, &clear)
            .expect("GPU map callback completed");
        let result = JsFuture::from(promise)
            .await
            .expect("GPU pixel readback map");
        // Tiny owned RGBA results only; BGRA-swizzle and finally-unmap are in the
        // same driver readback used by the standalone prototype qualification.
        let data = Uint8Array::new(&result);
        assert_eq!(data.length() as usize, N * 4, "mapped GPU pixel bytes");
        std::array::from_fn(|index| {
            std::array::from_fn(|channel| data.get_index((index * 4 + channel) as u32))
        })
    }
}

impl<const N: usize> Drop for PixelReadback<N> {
    fn drop(&mut self) {
        self.reader.dispose_readback();
        // Destroy target/output only, never renderer depth or device.
    }
}
