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

struct Batches {
    atlas: Vec<u8>,
    scratch: Vec<u8>,
    batches: usize,
}
impl AtlasSink for Batches {
    fn configure(&mut self, pages: u32) -> Result<(), String> {
        self.atlas.resize(pages as usize * GAME_ATLAS_PAGE_BYTES, 0);
        Ok(())
    }
    fn source_batches(&self) -> bool {
        true
    }
    fn write_source(
        &mut self,
        colour: &[u8],
        mask: Option<&[u8]>,
        copies: &[aoe_rendering::AtlasSourceCopy],
    ) -> Result<js_sys::Promise, String> {
        self.batches += 1;
        for copy in copies {
            for (layer, source) in [(Some(copy.colour), Some(colour)), (copy.mask, mask)] {
                let (Some(layer), Some(source)) = (layer, source) else {
                    continue;
                };
                for row in 0..usize::from(copy.size[1]) {
                    let from = ((usize::from(copy.source[1]) + row) * 2048
                        + usize::from(copy.source[0]))
                        * 4;
                    let to = usize::from(layer) * GAME_ATLAS_PAGE_BYTES
                        + ((usize::from(copy.destination[1]) + row) * 2048
                            + usize::from(copy.destination[0]))
                            * 4;
                    let width = usize::from(copy.size[0]) * 4;
                    self.atlas[to..to + width].copy_from_slice(&source[from..from + width]);
                }
            }
        }
        Ok(js_sys::Promise::resolve(&JsValue::UNDEFINED))
    }
    fn page_buffer(&mut self, _: u32) -> Result<&mut [u8], String> {
        self.scratch.clear();
        self.scratch.resize(GAME_ATLAS_PAGE_BYTES, 0);
        Ok(&mut self.scratch)
    }
    fn commit(&mut self, layer: u32) -> Result<(), String> {
        let start = layer as usize * GAME_ATLAS_PAGE_BYTES;
        self.atlas[start..start + 16384].copy_from_slice(&self.scratch[..16384]);
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[wasm_bindgen_test]
async fn source_batches_match_cpu_pixels_and_read_each_source_plane_once() {
    use aoe_assets::catalog::units::{Pose, SPRITES};
    let sheep = SPRITES[Pose::SheepStand as usize];
    let value = selection_tests::pack_fixture(
        REQUIRED_RENDER_SOURCES
            .iter()
            .map(|s| (s.manifest_source(), s.frames, s.terrain_topology.is_some()))
            .chain([(sheep.manifest_source(), sheep.frames(), false)]),
    );
    let fetch = selection_tests::FetchFixture::install_with_player(&value, true);
    let mut reference = CollectedAtlas(Vec::new());
    load(&mut reference).await.unwrap();
    let calls = || {
        js_sys::Array::from(
            &js_sys::Reflect::get(&web_sys::window().unwrap(), &"__aoeAssetRequests".into())
                .unwrap(),
        )
        .length()
    };
    let before = calls();
    let mut batches = Batches {
        atlas: Vec::new(),
        scratch: Vec::new(),
        batches: 0,
    };
    load(&mut batches).await.unwrap();
    // One source page: three planes plus the two manifest requests (one absent overlay).
    assert_eq!(calls() - before, 5);
    assert!(
        batches.batches >= 2,
        "owned and unmasked copies need distinct composition"
    );
    assert!(
        reference.0 == batches.atlas,
        "source-order GPU copies must preserve every CPU texel"
    );
    drop(fetch);
}
