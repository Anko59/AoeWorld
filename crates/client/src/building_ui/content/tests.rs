use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn native_static_pose_content_keeps_zero_duration_only_for_one_cell() {
    let mut value = fixture();
    let pose = aoe_assets::catalog::units::Pose::MangonelStand as usize;
    assert_eq!(value["actors"][pose][1], 1);
    assert_eq!(value["actors"][pose][2], 0);
    let content = decode(&serde_json::to_vec(&value).unwrap()).expect("native static content");
    assert_eq!(content.actors[pose].frame_millis(), 0.0);
    value["actors"][pose][1] = 2.into();
    assert!(decode(&serde_json::to_vec(&value).unwrap()).is_err());
}

fn fixture() -> serde_json::Value {
    serde_json::json!({
        "rules": aoe_simulation::browser_content(),
        "actors": aoe_assets::catalog::units::SPRITES.iter().map(|s|
            [s.id, u32::from(s.stride), u32::from(s.millis), u32::from(s.phases)]
        ).collect::<Vec<_>>()
    })
}

#[wasm_bindgen_test]
fn generated_content_preserves_native_actor_sampling_and_game_rules() {
    let data = serde_json::to_vec(&fixture()).unwrap();
    let content = decode(&data).unwrap();
    assert_eq!(content.rules.units.len(), aoe_protocol::UnitKind::ALL.len());
    for (actor, source) in content
        .actors
        .iter()
        .zip(aoe_assets::catalog::units::SPRITES)
    {
        assert_eq!(
            (actor.id, actor.stride, actor.millis, actor.phases),
            (source.id, source.stride, source.millis, source.phases)
        );
        assert_eq!(actor.selected_frames(), source.selected_frames());
    }
}

#[wasm_bindgen_test]
fn malformed_runtime_content_is_rejected_before_installation() {
    for bytes in [b"{}".as_slice(), b"{", &[255], &[b' '; 32769]] {
        assert!(decode(bytes).is_err());
    }
    for (section, index, value) in [
        ("rules", 0, serde_json::json!(0)),
        ("rules", 1, serde_json::json!(-1)),
        ("rules", 4, serde_json::json!(1.5)),
        ("actors", 0, serde_json::json!([993, 0, 1000, 0])),
        ("actors", 0, serde_json::json!([993, 6, 0, 0])),
        ("actors", 0, serde_json::json!([993, 6, 1000])),
        ("actors", 0, serde_json::json!([993, 255, 1000, 0])),
        ("actors", 0, serde_json::json!([993, 30, 1000, 0])),
        ("actors", 0, serde_json::json!([993, 30, 67, 1])),
        ("actors", 0, serde_json::json!([4500, 255, 17, 30])),
    ] {
        let mut data = fixture();
        data[section][index] = value;
        assert!(decode(&serde_json::to_vec(&data).unwrap()).is_err());
    }
}
