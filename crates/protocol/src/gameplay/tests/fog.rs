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
