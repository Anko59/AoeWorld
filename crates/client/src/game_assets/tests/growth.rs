use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn full_native_frames_grow_object_pairs_without_aliasing_base_masks() {
    let mut value = manifest::fixture();
    let mut f = value["frames"][0].clone();
    f["width"] = 2040.into();
    f["height"] = 2040.into();
    value["frames"] = (0..4)
        .map(|i| {
            let mut f = f.clone();
            f["frame"] = i.into();
            f
        })
        .collect::<Vec<_>>()
        .into();
    let manifest: Manifest = serde_json::from_value(value).unwrap();
    let selected: Vec<_> = (0..4).collect();
    let domains = vec![AtlasDomain::Objects; 4];
    let placed = placement::plan(&manifest, &selected, &domains).unwrap();
    assert_eq!(
        placed.iter().map(|p| p.page).collect::<Vec<_>>(),
        [2, 3, 4, 8]
    );
    assert_eq!(aoe_rendering::atlas_mask_page(u32::from(placed[3].page)), 9);
    let mut source = vec![0; GAME_ATLAS_PAGE_BYTES];
    source[..4].copy_from_slice(&[7, 255, 0, 255]);
    let mut destination = vec![0; GAME_ATLAS_PAGE_BYTES];
    let frame = &manifest.frames[3];
    let mut mask = placed[3];
    mask.page = 9;
    placement::copy_plane(
        frame,
        mask,
        placement::Plane::MaskPlayer,
        &source,
        &mut destination,
        [1, 2, 3],
    )
    .unwrap();
    let at = (usize::from(mask.y) * 2048 + usize::from(mask.x)) * 4;
    assert_eq!(&destination[at..at + 2], &[7, 255]);
    assert!(
        placement::copy_plane(
            frame,
            placed[3],
            placement::Plane::MaskPlayer,
            &source,
            &mut destination,
            [1, 2, 3]
        )
        .is_err()
    );
}
