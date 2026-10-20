use super::*;
use aoe_assets::catalog::units::{Pose, SPRITES};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn sheep_neutral_and_owned_frames_share_sources_but_have_distinct_player_colours() {
    let source = SPRITES[Pose::SheepStand as usize];
    let mut value = manifest::fixture();
    let template = value["frames"][0].clone();
    let frames = (0..source.frames())
        .map(|frame| {
            let mut f = template.clone();
            f["source"] = source.manifest_source().into();
            f["frame"] = frame.into();
            f
        })
        .collect::<Vec<_>>();
    value["frames"] = frames.into();
    let manifest: Manifest = serde_json::from_value(value.clone()).unwrap();
    let (mut selected, mut domains) = (Vec::new(), Vec::new());
    let ranges = actors::select(&manifest, &mut selected, &mut domains).unwrap();
    let owned = ranges[Pose::SheepStand as usize].as_ref().unwrap().start;
    let neutral = ranges[Pose::NeutralSheepStand as usize]
        .as_ref()
        .unwrap()
        .start;
    assert_ne!(owned, neutral);
    assert_eq!(selected[owned], selected[neutral]);
    assert_eq!(
        actors::colour(&ranges, owned, [65, 145, 245]),
        [65, 145, 245]
    );
    assert_eq!(
        actors::colour(&ranges, neutral, [65, 145, 245]),
        [190, 190, 190]
    );
    value["frames"].as_array_mut().unwrap().pop();
    let bad: Manifest = serde_json::from_value(value).unwrap();
    assert!(actors::select(&bad, &mut Vec::new(), &mut Vec::new()).is_err());
}
