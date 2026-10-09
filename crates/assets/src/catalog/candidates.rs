//! Discovery inventory, NOT approved render sources or a runtime load list.
//! Counts observed in private trial pack 7e6fa0da…7447ce; all ranges start at 0.
//! Candidate semantics are proposals, not established by numeric IDs or counts.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateSource {
    pub archive: &'static str,
    pub id: u32,
    pub observed_frames: u32,
    pub proposed_use: &'static str,
}

const fn terrain(id: u32, frames: u32, proposed_use: &'static str) -> CandidateSource {
    CandidateSource {
        archive: "terrain.drs",
        id,
        observed_frames: frames,
        proposed_use,
    }
}

const fn graphics(id: u32, frames: u32, proposed_use: &'static str) -> CandidateSource {
    CandidateSource {
        archive: "graphics.drs",
        id,
        observed_frames: frames,
        proposed_use,
    }
}

/// Metadata-confirmed contiguous ranges; no topology or gameplay approval yet.
/// In particular, 100 frames is NOT evidence of a seamless 10×10 sheet.
pub const UNAPPROVED_SOURCES: [CandidateSource; 19] = [
    graphics(2304, 9, "possible conifer shadows; pairing unverified"),
    terrain(15001, 100, "grass"),
    terrain(15009, 100, "grass"),
    terrain(15006, 100, "grass"),
    terrain(15017, 100, "beach"),
    terrain(15014, 100, "shallows"),
    terrain(15015, 100, "water"),
    terrain(15016, 100, "water"),
    terrain(15024, 64, "ice; topology unverified, not implicitly 8×8"),
    graphics(226, 25, "cliff; placeholder frames require review"),
    graphics(227, 25, "cliff; anomalous tall frame requires review"),
    graphics(228, 25, "cliff; placeholder frames require review"),
    graphics(229, 25, "cliff; placeholder frames require review"),
    graphics(230, 25, "cliff; placeholder frames require review"),
    graphics(231, 25, "cliff; placeholder frames require review"),
    graphics(232, 25, "cliff; placeholder frames require review"),
    graphics(233, 25, "cliff; placeholder frames require review"),
    graphics(234, 25, "cliff; placeholder frames require review"),
    graphics(235, 1, "cliff"),
];

#[cfg(test)]
mod tests {
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
        assert_eq!(UNAPPROVED_SOURCES.len(), 19);
        assert!(UNAPPROVED_SOURCES.iter().any(|source| source.id == 2304));
        assert!(
            !OPTIONAL_RESOURCE_SOURCES
                .iter()
                .any(|source| matches!(source.id, 2300 | 2304))
        );
        assert_eq!(
            UNAPPROVED_SOURCES
                .iter()
                .find(|source| source.id == 15024)
                .unwrap()
                .observed_frames,
            64
        );
    }
}
