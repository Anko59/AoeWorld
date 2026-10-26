use super::*;
fn change(revision: u64) -> aoe_map::Depletion {
    aoe_map::Depletion {
        id: 2,
        removed: 1,
        remaining: 10,
        became_nonblocking: false,
        revision,
    }
}
#[test]
fn retained_journal_deduplicates_and_lag_requires_snapshot() {
    let mut journal = Journal::default();
    for revision in 1..=1025 {
        journal.record(change(revision));
    }
    assert_eq!(journal.changes.len(), MAX_JOURNAL_CHANGES);
    assert!(journal.since(0, 1025).is_none());
    assert_eq!(journal.since(1, 1025).unwrap().len(), 1);
    assert!(journal.since(1025, 1025).is_none());
    journal.record(change(1027));
    assert!(journal.since(1025, 1027).is_none());
}

#[test]
fn reconnect_snapshots_over_65536_nodes_keep_each_encoded_frame_bounded() {
    use aoe_protocol::{
        MAX_RESOURCE_CACHE, MAX_RESOURCE_CHANGES, decode_gameplay_server, encode_gameplay_server,
    };
    assert_eq!(MAX_RESOURCE_CACHE, aoe_map::MAX_RESOURCE_OVERLAY_CHANGES);
    // Exercise both the first failing count and the complete supported overlay.
    for count in [MAX_RESOURCE_CHANGES + 1, MAX_RESOURCE_CACHE] {
        let source = ResourceState {
            subscription_revision: 7,
            from_revision: None,
            revision: count as u64,
            changes: (0..count)
                .map(|id| ResourceAmount {
                    id: id as u64 * 2,
                    remaining: 0,
                })
                .collect(),
            snapshot_page: None,
        };
        let frames = snapshot_frames(source.clone());
        let mut assembled = Vec::new();
        for state in frames {
            assert_eq!(
                state.snapshot_page,
                Some((assembled.len() as u32, count as u32))
            );
            assert!(state.changes.len() <= MAX_RESOURCE_CHANGES);
            let message = GameplayServerMessage::ResourceState(state.clone());
            let bytes = encode_gameplay_server(&message).expect("bounded snapshot page");
            assert_eq!(decode_gameplay_server(&bytes).unwrap(), message);
            assembled.extend(state.changes);
        }
        assert_eq!(assembled, source.changes);
    }
}

#[test]
fn the_complete_supported_overlay_roundtrips_through_bounded_storage() {
    let count = aoe_map::MAX_RESOURCE_OVERLAY_CHANGES;
    let snapshot = aoe_map::ResourceOverlaySnapshot {
        schema_version: aoe_map::RESOURCE_OVERLAY_SCHEMA_VERSION,
        map_content_hash: [7; 32],
        revision: count as u64,
        changes: (0..count)
            .map(|id| aoe_map::ResourceChange {
                id: id as u64 * 2,
                remaining: u16::MAX,
            })
            .collect(),
    };
    let directory = tempfile::tempdir().unwrap();
    super::super::store::save(directory.path(), &snapshot, 0).unwrap();
    assert_eq!(
        super::super::store::load(directory.path(), snapshot.map_content_hash).unwrap(),
        Some(snapshot)
    );
}
