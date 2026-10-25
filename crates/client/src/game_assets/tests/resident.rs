//! The CPU source-order path must produce the same bytes as page streaming.
use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

struct Streamed(CollectedAtlas);

impl AtlasSink for Streamed {
    fn page_buffer(&mut self, index: u32) -> Result<&mut [u8], String> {
        self.0.page_buffer(index)
    }
    fn commit(&mut self, index: u32) -> Result<(), String> {
        self.0.commit(index)
    }
    fn finish(&mut self) -> Result<(), String> {
        self.0.finish()
    }
}

#[wasm_bindgen_test]
async fn resident_source_order_preserves_every_streamed_atlas_byte() {
    let value = selection_tests::pack_fixture(REQUIRED_RENDER_SOURCES.iter().map(|source| {
        (
            source.manifest_source(),
            source.frames,
            source.terrain_topology.is_some(),
        )
    }));
    // Partial base alpha and a nonzero owner ramp catch mask-plane ordering.
    let fetch = selection_tests::FetchFixture::install_with_player(&value, true);
    let mut resident = CollectedAtlas(Vec::new());
    load(&mut resident).await.unwrap();
    let mut streamed = Streamed(CollectedAtlas(Vec::new()));
    load(&mut streamed).await.unwrap();
    drop(fetch);
    assert_eq!(resident.0.len(), GAME_ATLAS_BYTES);
    assert_eq!(streamed.0.0.len(), GAME_ATLAS_BYTES);
    let mismatch = resident
        .0
        .iter()
        .zip(&streamed.0.0)
        .position(|(a, b)| a != b);
    assert_eq!(mismatch, None, "CPU and WebGPU upload texels must agree");
}
