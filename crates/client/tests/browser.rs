#![cfg(target_arch = "wasm32")]

use aoe_client::replay_hash;
use aoe_protocol::{ClientMessage, VERSION, decode_client, encode_client};
use aoe_scenario::SMOKE;
use aoe_simulation::World;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn replay_hash_matches_shared_world_inside_browser() {
    let mut world = World::new(SMOKE);
    for _ in 0..4 {
        world.advance();
    }
    assert_eq!(
        replay_hash("smoke", 4).expect("WASM replay"),
        world.canonical_hash_hex()
    );
    assert!(replay_hash("missing", 0).is_err());
    assert!(replay_hash("smoke", 65).is_err());
}

#[wasm_bindgen_test]
fn protocol_round_trip_runs_in_browser() {
    let message = ClientMessage::Hello { version: VERSION };
    let bytes = encode_client(&message).expect("encode");
    assert_eq!(decode_client(&bytes).expect("decode"), message);
    assert!(
        web_sys::window()
            .and_then(|window| window.document())
            .is_some()
    );
}
