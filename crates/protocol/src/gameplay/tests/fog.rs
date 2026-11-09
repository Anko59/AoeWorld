use super::*;

#[test]
fn appended_fog_frame_matches_browser_golden_and_round_trips() {
    let message = ServerMessage::FogState(FogState {
        revision: 1,
        tick: Tick(2),
        region: TileRect::from_xywh(10, 20, 3, 1),
        disabled: false,
        tiles: vec![0, 1, 2],
        overview: vec![(257, 1), (258, 2)],
    });
    let bytes = encode_server(&message).unwrap();
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex,
        include_str!("../../../../../browser/tests/wire/fog.hex").trim()
    );
    assert_eq!(decode_server(&bytes).unwrap(), message);
}

/// The changes that bring a held 7x3 view to the next one: tile bytes 0 to 2
/// in one run (byte 1 is unchanged and carried), then, two unchanged bytes on,
/// the last byte, which holds only the window's last tile; and one strategic
/// pixel that changed, one that went black and one that appeared.
#[test]
fn appended_fog_delta_frame_matches_browser_golden_and_round_trips() {
    let held = FogState {
        revision: 1,
        tick: Tick(2),
        region: TileRect::from_xywh(10, 20, 7, 3),
        disabled: false,
        tiles: vec![
            0, 0, 1, 2, 1, 1, 1, 1, 2, 2, 0, 0, 0, 0, 0, 0, 1, 2, 1, 2, 0,
        ],
        overview: vec![(257, 1), (258, 2)],
    };
    let fresh = FogState {
        tick: Tick(3),
        tiles: vec![
            1, 2, 2, 2, 1, 1, 1, 1, 2, 2, 2, 1, 0, 0, 0, 0, 1, 2, 1, 2, 2,
        ],
        overview: vec![(257, 2), (300, 1)],
        ..held.clone()
    };
    let delta = held.delta_to(&fresh).unwrap();
    let message = ServerMessage::FogDelta(delta.clone());
    let bytes = encode_server(&message).unwrap();
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        hex,
        include_str!("../../../../../browser/tests/wire/fog-delta.hex").trim()
    );
    assert_eq!(decode_server(&bytes).unwrap(), message);
    let mut client = held;
    client.apply(&delta).unwrap();
    assert_eq!(client, fresh);
}

#[test]
fn fog_rejects_invalid_tiles_shapes_and_unbounded_vectors() {
    let mut state = FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::from_xywh(0, 0, 1, 1),
        disabled: false,
        tiles: vec![2],
        overview: vec![],
    };
    assert!(encode_server(&ServerMessage::FogState(state.clone())).is_ok());
    state.tiles[0] = 3;
    assert!(encode_server(&ServerMessage::FogState(state.clone())).is_err());
    state.tiles = vec![0; 262_145];
    let raw = postcard::to_allocvec(&ServerMessage::FogState(state)).unwrap();
    assert!(decode_server(&raw).is_err());
    let impossible = FogState {
        revision: 1,
        tick: Tick(0),
        region: TileRect::new(TileCoord::new(i32::MIN, 0), TileCoord::new(i32::MAX, 1)),
        disabled: true,
        tiles: vec![],
        overview: vec![],
    };
    assert!(encode_server(&ServerMessage::FogState(impossible)).is_err());
}
