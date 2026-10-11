use super::*;
use crate::catalog::{
    OPTIONAL_RESOURCE_SOURCES, OPTIONAL_TERRAIN_SOURCES, REQUIRED_RENDER_SOURCES,
};

#[test]
fn discovery_is_unique_bounded_and_not_a_reviewed_load_list() {
    let mut ids = std::collections::BTreeSet::new();
    for candidate in UNAPPROVED_SOURCES {
        assert!(ids.insert((candidate.archive, candidate.id)));
        assert!((1..=100).contains(&candidate.observed_frames));
        assert!(
            !REQUIRED_RENDER_SOURCES
                .iter()
                .chain(OPTIONAL_RESOURCE_SOURCES.iter())
                .chain(OPTIONAL_TERRAIN_SOURCES.iter())
                .any(|source| source.archive == candidate.archive && source.id == candidate.id)
        );
    }
    assert_eq!(UNAPPROVED_SOURCES.len(), 10);
    assert!(UNAPPROVED_SOURCES.iter().any(|source| source.id == 2304));
    assert!(
        !OPTIONAL_RESOURCE_SOURCES
            .iter()
            .any(|source| matches!(source.id, 2300 | 2304))
    );
    assert!(!UNAPPROVED_SOURCES.iter().any(|source| source.id == 15024));
}
