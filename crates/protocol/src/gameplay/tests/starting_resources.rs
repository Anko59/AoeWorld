use super::*;
use crate::{GameplayServerMessage, decode_gameplay_server, encode_gameplay_server};

#[test]
fn starting_resource_identity_roundtrips_on_the_current_resource_wire() {
    // Starting gold, tile (12, 13): namespace bit 37, kind bits 38..39.
    let id = (1_u64 << 37) | (2_u64 << 38) | (13 << 19) | (12 << 1);
    let message = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![ResourceAmount { id, remaining: 800 }],
    });
    let encoded = encode_gameplay_server(&message).expect("resource wire");
    assert_eq!(
        decode_gameplay_server(&encoded).expect("resource wire"),
        message
    );
    // Current wire appends the snapshot-page option after resource changes.
    assert_eq!(
        encoded,
        [5, 1, 0, 0, 1, 152, 128, 160, 131, 128, 20, 160, 6, 0]
    );
    let mut invalid = message;
    let GameplayServerMessage::ResourceState(ref mut state) = invalid else {
        unreachable!()
    };
    state.changes[0].id |= 1 << 40;
    assert!(encode_gameplay_server(&invalid).is_err());
}

#[test]
fn starting_tree_family_is_bounded_and_survives_the_wire() {
    for family in 1..=4_u64 {
        let id = (family << 40) | (1 << 38) | (1 << 37) | (13 << 19) | (12 << 1);
        let message = GameplayServerMessage::ResourceState(ResourceState {
            snapshot_page: None,
            subscription_revision: 1,
            from_revision: None,
            revision: 0,
            changes: vec![ResourceAmount { id, remaining: 100 }],
        });
        let bytes = encode_gameplay_server(&message).expect("tree family");
        assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
        if family == 4 {
            assert_eq!(
                bytes,
                [5, 1, 0, 0, 1, 152, 128, 160, 131, 128, 140, 1, 100, 0]
            );
        }
        for bad_id in [
            id | 1,
            id | (1 << 43),
            id & ((1 << 40) - 1),
            (id & ((1 << 40) - 1)) | (5 << 40),
        ] {
            let GameplayServerMessage::ResourceState(mut state) = message.clone() else {
                unreachable!()
            };
            state.changes[0].id = bad_id;
            assert!(!state.valid(), "invalid identity {bad_id}");
        }
    }
}
