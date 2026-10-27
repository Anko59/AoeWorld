use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn siege_partial_rows_draw_the_native_body_delta() {
    let frame = |number| GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [number as f32 / 4096.0, 0.0, 0.01, 0.01],
        },
        size: [24.0, 45.0],
        anchor: [12.0, 40.0],
    };
    let mut art = art();
    art.actors = vec![Vec::new(); 133];
    for (kind, body_kind, base, body) in [
        (
            SceneUnitKind::MangonelWalk,
            SceneUnitKind::MangonelStand,
            1100.0,
            1200.0,
        ),
        (
            SceneUnitKind::BatteringRamWalk,
            SceneUnitKind::BatteringRamStand,
            1300.0,
            1400.0,
        ),
        (
            SceneUnitKind::BatteringRamAttack,
            SceneUnitKind::BatteringRamStand,
            1500.0,
            1600.0,
        ),
    ] {
        art.actors[kind.actor_slot().unwrap()] = vec![frame(base); 50];
        art.actors[body_kind.actor_slot().unwrap()] = vec![frame(body); 5];
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[unit(kind)], camera(), 0);
        assert_eq!(
            frames.len(),
            3,
            "{kind:?} draws shadow, wheel/log and body: {:?}",
            frames.iter().map(|f| f.1.atlas.uv[0]).collect::<Vec<_>>()
        );
        assert_eq!(frames[1].1.atlas.uv[0], base / 4096.0);
        assert_eq!(frames[2].1.atlas.uv[0], body / 4096.0);
    }
}
