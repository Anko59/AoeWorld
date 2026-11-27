use super::review_tests::{client_with_backend, contact_scene};
use crate::playground::game::village::relics;
use crate::unit_visuals::Pose;
use aoe_core::EntityId;
use aoe_protocol::{GroundRelic, RelicView, UnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn visible_relics_and_monk_carry_art_follow_the_subscription() {
    let scene = contact_scene();
    let mut client = client_with_backend(&scene, Some("canvas")).await;
    let id = EntityId(1);
    let monk = client.units.get_mut(&id).unwrap();
    monk.kind = UnitKind::Monk;
    let ground = GroundRelic {
        id: EntityId(20),
        position: monk.position,
    };
    client.revision = 7;
    relics::sync(
        &mut client,
        RelicView {
            revision: 7,
            ground: vec![ground],
            carriers: vec![id],
            ..RelicView::default()
        },
    );
    assert_eq!(relics::ground(&client), [ground]);
    assert_eq!(
        super::actors::shown_pose(&client, &client.units[&id]).1,
        Pose::MonkWithRelicStand
    );
    client.revision += 1;
    assert!(relics::ground(&client).is_empty());
    assert!(!relics::carries(&client, id));
    client.canvas.remove();
}
