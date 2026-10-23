use super::*;
use crate::{
    GAMEPLAY_MAX_MESSAGE, GameplayServerMessage, decode_gameplay_server, encode_gameplay_server,
};
#[test]
fn resource_wire_fixture_and_full_limit_fit_the_frame_bound() {
    let message = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 1,
        changes: vec![ResourceAmount {
            id: 0,
            remaining: 0,
        }],
    });
    // The current wire appends the explicit snapshot-page option.
    assert_eq!(
        encode_gameplay_server(&message).unwrap(),
        [6, 1, 0, 1, 1, 0, 0, 0]
    );
    assert_eq!(
        decode_gameplay_server(&[6, 1, 0, 1, 1, 0, 0, 0]).unwrap(),
        message
    );
    let changes = (0..MAX_RESOURCE_CHANGES)
        .map(|index| ResourceAmount {
            id: (1_u64 << 36) + index as u64 * 2,
            remaining: u16::MAX,
        })
        .collect();
    let full = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: u64::MAX,
        from_revision: None,
        revision: u64::MAX,
        changes,
    });
    let bytes = encode_gameplay_server(&full).unwrap();
    assert!(bytes.len() < GAMEPLAY_MAX_MESSAGE);
    assert_eq!(decode_gameplay_server(&bytes).unwrap(), full);
}
#[test]
fn malformed_and_oversized_resource_frames_are_rejected_on_decode() {
    let empty = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(1),
        revision: 2,
        changes: Vec::new(),
    });
    assert!(encode_gameplay_server(&empty).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&empty).unwrap()).is_err());
    let invalid = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(3),
        revision: 2,
        changes: Vec::new(),
    });
    assert!(encode_gameplay_server(&invalid).is_err());
    assert!(decode_gameplay_server(&postcard::to_allocvec(&invalid).unwrap()).is_err());
    let oversized = GameplayServerMessage::ResourceState(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: u64::MAX,
        changes: vec![
            ResourceAmount {
                id: 0,
                remaining: 0
            };
            MAX_RESOURCE_CHANGES + 1
        ],
    });
    assert!(decode_gameplay_server(&postcard::to_allocvec(&oversized).unwrap()).is_err());
}
