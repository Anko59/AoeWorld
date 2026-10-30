use super::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen_test]
async fn verified_native_chunks_upload_on_demand_and_resident_cells_do_not_upload_again() {
    let planes = [
        vec![117, 120, 115, 128],
        vec![7, 0, 0, 119],
        vec![0, 0, 0, 77],
    ];
    let bytes = compressed::encode_frame([1, 1], planes.each_ref().map(Vec::as_slice)).unwrap();
    let chunk = FrameChunk::from_bytes("frames-0000.bin".into(), &bytes).unwrap();
    let mut native = NativeFrames::new(vec![chunk], vec![cell(0, bytes.len() as u32)], 8).unwrap();
    // One pair exercises the production residency/upload path with bounded
    // synthetic Canvas storage; constructor tests cover the normal four pairs.
    native.cache = FrameCache::new(8, 1, 64).unwrap();
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    document.body().unwrap().append_child(&canvas).unwrap();
    let (mut renderer, canvas) =
        aoe_rendering::GameRenderer::new_with_backend(canvas, Some("canvas"))
            .await
            .unwrap();
    let mut upload = renderer
        .begin_game_atlas(u32::from(native.layer_count()))
        .unwrap();
    for layer in 0..u32::from(native.layer_count()) {
        upload.page_buffer(layer).unwrap();
        renderer.write_game_atlas_page(&mut upload, layer).unwrap();
    }
    renderer.finish_game_atlas(upload).unwrap();
    let mut art = empty_art();
    let (changed, requests) = native.update(&[1], &[], &mut art, &mut renderer).unwrap();
    assert!(!changed);
    assert_eq!(requests.len(), 1);
    assert!(art.resident[0].is_none());
    native.accept(0, bytes).unwrap();
    let (changed, requests) = native.update(&[1], &[], &mut art, &mut renderer).unwrap();
    assert!(changed);
    assert!(requests.is_empty());
    let address = art.resident[0].unwrap();
    assert_eq!(address.page, 8);
    let aoe_rendering::GameRenderer::Canvas { source_atlas, .. } = &renderer else {
        unreachable!()
    };
    let at = 8 * GAME_ATLAS_PAGE_BYTES + (2 * GAME_ATLAS_SIDE as usize + 2) * 4;
    assert_eq!(&source_atlas[at..at + 4], &[117, 120, 115, 187]);
    assert_eq!(
        &source_atlas[at + GAME_ATLAS_PAGE_BYTES..at + GAME_ATLAS_PAGE_BYTES + 4],
        &[7, 119, 128, 0]
    );
    let (changed, requests) = native.update(&[1], &[], &mut art, &mut renderer).unwrap();
    assert!(!changed);
    assert!(requests.is_empty());
    assert_eq!(art.resident[0], Some(address));
    canvas.remove();
}

fn empty_art() -> GameArt {
    GameArt {
        resident: vec![None],
        actors: Vec::new(),
        actor_rows: Vec::new(),
        buildings: std::array::from_fn(|_| Vec::new()),
        fishing: Default::default(),
        village: Default::default(),
        compositions: Vec::new(),
        foundation: Default::default(),
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: Default::default(),
    }
}
