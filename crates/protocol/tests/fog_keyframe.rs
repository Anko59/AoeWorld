//! A fog keyframe: the whole view of one subscription, as a client receives
//! it on subscribe and whenever its window or the fog mode changes.
use aoe_core::{Tick, TileRect};
use aoe_protocol::{
    FogState, GameplayServerMessage as M, decode_gameplay_server, encode_gameplay_server,
};

/// Header and lengths of one keyframe without strategic pixels.
const FRAMING: usize = 64;

/// Deterministic noise over the three states: no run for a coder to shorten.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % 3) as u8
        })
        .collect()
}

fn keyframe(width: i32, height: i32, tiles: Vec<u8>, overview: Vec<(u16, u8)>) -> M {
    M::FogState(FogState {
        revision: 3,
        tick: Tick(41),
        region: TileRect::from_xywh(120, 300, width, height),
        disabled: false,
        tiles,
        overview,
    })
}

#[test]
fn a_keyframe_costs_two_bits_per_tile_whatever_the_view_holds() {
    // The measured view, an odd one, the late-game camera and the largest.
    let sizes = [(96, 96), (97, 95), (157, 157), (512, 512)].map(|(width, height)| {
        let tiles = (width * height) as usize;
        let message = keyframe(width, height, noise(7, tiles), Vec::new());
        let bytes = encode_gameplay_server(&message).unwrap().len();
        (width, height, bytes, tiles.div_ceil(4) + FRAMING)
    });
    assert!(
        sizes.iter().all(|(_, _, bytes, budget)| bytes <= budget),
        "keyframes as (width, height, bytes, budget at two bits per tile): {sizes:?}"
    );
}

#[test]
fn keyframes_of_every_shape_survive_the_wire() {
    // Tile counts that leave one, two, three or four tiles in a last byte.
    let shapes = [
        (1, 1),
        (2, 1),
        (1, 3),
        (2, 2),
        (5, 1),
        (3, 5),
        (7, 9),
        (97, 95),
        (512, 1),
        (1, 512),
    ];
    for (width, height) in shapes {
        let len = (width * height) as usize;
        let views = [
            noise(1, len),
            noise(2, len),
            vec![0; len],
            vec![1; len],
            vec![2; len],
        ];
        for tiles in views {
            let overview = vec![(0, 1), (300, 2), (65_535, 1)];
            let message = keyframe(width, height, tiles, overview);
            let bytes = encode_gameplay_server(&message).unwrap();
            assert_eq!(
                decode_gameplay_server(&bytes).unwrap(),
                message,
                "{width} x {height}"
            );
        }
    }
}

#[test]
fn a_truncated_keyframe_is_rejected() {
    let overview = vec![(7, 2), (40_000, 1)];
    let message = keyframe(7, 9, noise(5, 63), overview);
    let bytes = encode_gameplay_server(&message).unwrap();
    for cut in 0..bytes.len() {
        assert!(
            decode_gameplay_server(&bytes[..cut]).is_err(),
            "a keyframe cut to {cut} of {} bytes decoded",
            bytes.len()
        );
    }
}
