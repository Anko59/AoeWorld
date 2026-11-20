use super::*;
use aoe_core::Tick;
use aoe_protocol::{UnitKind, UnitTask};
use aoe_rendering::{AtlasAddress, GameFrame, SceneUnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn petard_detonation_plays_the_particle_without_leaving_a_body() {
    let scene = PreparedScene::new(Camera::new([10.0, 10.0], [512.0; 2]), Vec::new());
    let mut client = review_tests::client_with_backend(&scene, Some("canvas")).await;
    let id = client.primary.unwrap();
    let unit = client.units.get_mut(&id).unwrap();
    unit.kind = UnitKind::Petard;
    unit.hp = 0;
    unit.task = UnitTask::Dying;
    unit.death_tick = Some(Tick(100));
    let slot = crate::unit_visuals::Pose::PetardBlast as usize;
    client.art.actors.resize_with(slot + 1, Vec::new);
    client.art.actors[slot].push(GameFrame {
        stream: 0,
        atlas: AtlasAddress::WHITE,
        size: [1.0; 2],
        anchor: [0.0; 2],
    });

    client.config.tick_hz = 20;
    client.presentation.reset(20);
    client.presentation.receive(105, 0.0);
    client.village.tick = 105;
    let scene_units = units(&client, &scene);
    assert_eq!(scene_units.len(), 1);
    assert_eq!(scene_units[0].kind, SceneUnitKind::PetardBlast);

    client.presentation.receive(131, 0.0);
    client.village.tick = 131;
    assert!(units(&client, &scene).is_empty());
    client.canvas.remove();
}
