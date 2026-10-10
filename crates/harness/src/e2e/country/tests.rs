use super::*;
use aoe_core::{PlayerId, Tick};
use aoe_protocol::{
    CommandResult as GameplayCommandResult, GameplayUnitState, encode_gameplay_client,
    encode_gameplay_server,
};

fn server_frame(message: GameplayServerMessage) -> WireFrame {
    WireFrame {
        direction: "received".to_owned(),
        payload_hex: encode_gameplay_server(&message)
            .expect("encode bounded server fixture")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn client_frame(message: GameplayClientMessage) -> WireFrame {
    WireFrame {
        direction: "sent".to_owned(),
        payload_hex: encode_gameplay_client(&message)
            .expect("encode bounded client fixture")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn persist_capture(directory: &tempfile::TempDir, capture: &WireCapture) {
    fs::write(
        directory.path().join("live-wire.json"),
        serde_json::to_vec(capture).expect("serialize wire fixture"),
    )
    .expect("persist wire fixture");
}

fn read_capture(directory: &tempfile::TempDir) -> WireCapture {
    serde_json::from_slice(
        &fs::read(directory.path().join("live-wire.json")).expect("wire fixture"),
    )
    .expect("valid wire fixture")
}

fn wire_fixture(ack: GameplayCommandResult, arrive: bool) -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().expect("wire evidence directory");
    let hash = "ab".repeat(32);
    let primary = EntityId(7);
    let origin = WorldPosition::from_tile_center(TileCoord::new(9, 12)).expect("origin tile");
    let target_tile = TileCoord::new(10, 12);
    let target = WorldPosition::from_tile_center(target_tile).expect("target tile");
    let middle = WorldPosition::new(target.x - 1, target.y);
    let messages = vec![
        server_frame(GameplayServerMessage::Welcome {
            version: GAMEPLAY_VERSION,
            world_id: 1,
            map_content_hash: Some([0xab; 32]),
            map_metadata: None,
            width_tiles: 20,
            height_tiles: 20,
            coordinate_precision: 1024,
            tick_hz: 20,
            role: GameplayRole::Controller,
            primary_unit_id: primary,
            resume_token: None,
        }),
        server_frame(GameplayServerMessage::Snapshot {
            revision: 0,
            tick: Tick(0),
            units: vec![GameplayUnitState {
                id: primary,
                player: PlayerId(0),
                position: origin,
                moving: false,
                planning: false,
                facing: 0,
            }],
        }),
        client_frame(GameplayClientMessage::MoveOrder {
            sequence: 1,
            entity_id: primary,
            destination: target,
        }),
        server_frame(GameplayServerMessage::CommandAck {
            sequence: 1,
            result: ack,
            applied_tick: Tick(1),
        }),
        server_frame(GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(1),
            changed_units: vec![GameplayUnitState {
                id: primary,
                player: PlayerId(0),
                position: middle,
                moving: true,
                planning: false,
                facing: 0,
            }],
            removals: Vec::new(),
        }),
        server_frame(GameplayServerMessage::Tick {
            revision: 2,
            tick: Tick(2),
            changed_units: vec![GameplayUnitState {
                id: primary,
                player: PlayerId(0),
                position: if arrive { target } else { middle },
                moving: !arrive,
                planning: false,
                facing: 0,
            }],
            removals: Vec::new(),
        }),
    ];
    let payload_bytes = messages
        .iter()
        .map(|frame| frame.payload_hex.len() / 2)
        .sum();
    let capture = WireCapture {
        content_hash: hash.clone(),
        destination_tile: [target_tile.x, target_tile.y],
        frame_count: messages.len(),
        payload_bytes,
        frames: messages,
    };
    persist_capture(&directory, &capture);
    (directory, hash)
}

#[test]
fn candidate_hash_validation_does_not_accept_paths_or_uppercase() {
    assert!(validate_hash(&"a".repeat(64)).is_ok());
    for value in [
        "",
        "../manifest",
        &"F".repeat(64),
        &"g".repeat(64),
        &"0".repeat(63),
    ] {
        assert!(validate_hash(value).is_err());
    }
}

#[test]
fn live_wire_requires_versioned_controller_ack_motion_and_exact_arrival() {
    let (directory, hash) = wire_fixture(GameplayCommandResult::Accepted, true);
    let evidence = validate_live_wire(directory.path(), &hash)
        .expect("protocol frames prove exact controller arrival");
    assert_eq!(evidence["arrived_idle_at_exact_target"], true);
    assert_eq!(evidence["accepted"], true);
    assert_eq!(evidence["authoritative_position_observations"], 3);
}

#[test]
fn live_wire_rejects_failed_acknowledgment_or_missing_arrival() {
    let (directory, hash) = wire_fixture(GameplayCommandResult::RejectedUnreachable, true);
    assert!(validate_live_wire(directory.path(), &hash).is_err());
    let (directory, hash) = wire_fixture(GameplayCommandResult::Accepted, false);
    assert!(validate_live_wire(directory.path(), &hash).is_err());
}

#[test]
fn live_wire_requires_ordered_motion_of_the_welcomed_primary() {
    let (directory, hash) = wire_fixture(GameplayCommandResult::Accepted, true);
    let mut capture = read_capture(&directory);
    let mut frames = capture.frames.into_iter();
    let welcome = frames.next().expect("welcome");
    let snapshot = frames.next().expect("snapshot");
    let order = frames.next().expect("move order");
    let ack = frames.next().expect("ack");
    let motion = frames.next().expect("motion");
    let arrival = frames.next().expect("arrival");
    capture.frames = vec![welcome, snapshot, motion, order, ack, arrival];
    persist_capture(&directory, &capture);
    assert!(validate_live_wire(directory.path(), &hash).is_err());

    let (directory, hash) = wire_fixture(GameplayCommandResult::Accepted, true);
    let mut capture = read_capture(&directory);
    capture.frames[2] = client_frame(GameplayClientMessage::MoveOrder {
        sequence: 1,
        entity_id: EntityId(8),
        destination: WorldPosition::from_tile_center(TileCoord::new(10, 12))
            .expect("destination tile"),
    });
    persist_capture(&directory, &capture);
    assert!(validate_live_wire(directory.path(), &hash).is_err());
}

#[test]
fn live_wire_rejects_bad_hex_and_inconsistent_frame_accounting() {
    assert!(decode_hex("abc").is_err());
    assert!(decode_hex("zz").is_err());
    let (directory, hash) = wire_fixture(GameplayCommandResult::Accepted, true);
    let mut capture: WireCapture = serde_json::from_slice(
        &fs::read(directory.path().join("live-wire.json")).expect("fixture"),
    )
    .expect("valid fixture");
    capture.payload_bytes += 1;
    fs::write(
        directory.path().join("live-wire.json"),
        serde_json::to_vec(&capture).expect("serialize malformed fixture"),
    )
    .expect("write malformed fixture");
    assert!(validate_live_wire(directory.path(), &hash).is_err());
}
