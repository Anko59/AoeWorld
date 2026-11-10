use super::tests::{art, camera, unit};
use super::*;
use aoe_core::EntityId;
use wasm_bindgen_test::wasm_bindgen_test;

/// One drawn cell in each of the sixteen directions of a native actor.
fn art_with_archer() -> GameArt {
    let mut art = art();
    art.actors = vec![Vec::new(); 135];
    art.actors[SceneUnitKind::ArcherStand.actor_slot().unwrap()] = (0..16)
        .map(|i| GameFrame {
            stream: 0,
            atlas: crate::AtlasAddress::new(8, [i as f32 / 2048.0, 0.0, 0.01, 0.02]),
            size: [20.0, 40.0],
            anchor: [10.0, 40.0],
        })
        .collect();
    art
}

fn emitted(frames: &[(Sprite, GameFrame, f64, u64)], id: u64) -> Vec<f64> {
    frames.iter().filter(|f| f.3 == id).map(|f| f.2).collect()
}

#[wasm_bindgen_test]
fn an_actor_on_a_bridge_deck_stays_in_front_of_the_flat_piece_at_every_corner() {
    let art = art_with_archer();
    let mut deck = unit(SceneUnitKind::building(66));
    deck.id = EntityId(10);
    deck.position = [4.5, 4.5];
    let deck_depth = super::super::scene_unit_depth(deck);
    // The far corner of the tile is a whole depth unit shallower than its centre,
    // which left the actor behind the deck that it stands on.
    for corner in [[4.02, 4.02], [4.98, 4.02], [4.02, 4.98], [4.98, 4.98]] {
        let mut actor = unit(SceneUnitKind::ArcherStand);
        actor.id = EntityId(11);
        actor.position = corner;
        let frames = super::super::world_sprite_frames(&art, &[], &[], &[deck, actor], camera(), 0);
        let depths = emitted(&frames, 11);
        assert!(!depths.is_empty(), "actor drawn at {corner:?}");
        assert!(
            depths.iter().all(|d| *d > deck_depth),
            "{corner:?} behind the deck: {depths:?} vs {deck_depth}"
        );
    }
}

#[wasm_bindgen_test]
fn an_actor_off_the_bridge_keeps_its_ground_depth() {
    let art = art_with_archer();
    let mut deck = unit(SceneUnitKind::building(66));
    deck.id = EntityId(10);
    deck.position = [4.5, 4.5];
    let mut actor = unit(SceneUnitKind::ArcherStand);
    actor.id = EntityId(11);
    actor.position = [6.5, 4.5];
    let frames = super::super::world_sprite_frames(&art, &[], &[], &[deck, actor], camera(), 0);
    for depth in emitted(&frames, 11) {
        assert_eq!(depth, super::super::scene_unit_depth(actor));
    }
}
