//! Fog deltas in the browser's fog holder. These run natively: the wasm
//! client is asked to apply deltas through `aoe-protocol`'s `FogState::apply`,
//! the code these and the protocol's own tests cover.
//!
//! API asked of the client: `Fog::apply_delta(&mut self, &FogDelta) -> bool`,
//! true when the held view changed; a delta that does not fit is refused.
use crate::fog::Fog;
use aoe_core::{Tick, TileRect};
use aoe_protocol::{
    FogDelta, FogState, GameplayServerMessage as M, decode_gameplay_server, encode_gameplay_server,
};

const FIRST: [u8; 15] = [0, 0, 1, 2, 2, 0, 1, 2, 2, 2, 0, 0, 1, 1, 2];
const SECOND: [u8; 15] = [0, 1, 1, 1, 2, 0, 1, 1, 2, 2, 0, 0, 1, 2, 2];

/// A 5 x 3 window at (10, 20).
fn view(tiles: [u8; 15]) -> FogState {
    FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::from_xywh(10, 20, 5, 3),
        disabled: false,
        tiles: tiles.to_vec(),
        overview: vec![(7, 1)],
    }
}

/// The delta as it reaches the browser: built as the server builds it, then
/// through the wire.
fn received(held: &FogState, fresh: &FogState) -> FogDelta {
    let delta = held.delta_to(fresh).expect("the same window");
    let bytes = encode_gameplay_server(&M::FogDelta(delta)).unwrap();
    match decode_gameplay_server(&bytes).unwrap() {
        M::FogDelta(delta) => delta,
        other => panic!("a fog delta decoded as {other:?}"),
    }
}

fn states(fog: &Fog) -> Vec<u8> {
    (0..15)
        .map(|index| fog.state([10 + index % 5, 20 + index / 5]))
        .collect()
}

#[test]
fn a_delta_changes_the_held_view_tile_for_tile() {
    let (first, second) = (view(FIRST), view(SECOND));
    let mut fog = Fog::default();
    assert!(fog.apply(first.clone()));
    assert!(
        fog.apply_delta(&received(&first, &second)),
        "the shading changed"
    );
    assert_eq!(states(&fog), SECOND);
    assert_eq!(fog.state([9, 20]), 0, "outside the window stays black");
    assert_eq!(fog.brightness([13.5, 20.5]), 0.45, "out of sight: dimmed");
    // And back, from the view the client now holds.
    assert!(fog.apply_delta(&received(&second, &first)));
    assert_eq!(states(&fog), FIRST);
}

#[test]
fn a_delta_without_the_view_it_was_made_for_is_refused() {
    let (first, second) = (view(FIRST), view(SECOND));
    let delta = received(&first, &second);
    let mut fog = Fog::default();
    assert!(!fog.apply_delta(&delta), "nothing is held yet");
    assert_eq!(states(&fog), [0; 15], "still black");

    assert!(fog.apply(FogState {
        revision: 2,
        ..first.clone()
    }));
    assert!(!fog.apply_delta(&delta), "another subscription revision");
    assert_eq!(states(&fog), FIRST, "the held view is untouched");

    assert!(fog.apply(FogState {
        disabled: true,
        tiles: Vec::new(),
        overview: Vec::new(),
        ..first.clone()
    }));
    assert!(!fog.apply_delta(&delta), "fog is switched off");
    assert!(fog.disabled());

    fog.clear();
    assert!(!fog.apply_delta(&delta), "cleared for a new connection");
    assert_eq!(states(&fog), [0; 15]);
}
