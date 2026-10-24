use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::fmt;

/// Entries in one bounded wire message.
pub const MAX_RESOURCE_CHANGES: usize = 65_536;
/// Entries retained across snapshot pages and deltas; matches the map overlay.
pub const MAX_RESOURCE_CACHE: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceAmount {
    pub id: u64,
    pub remaining: u16,
}

/// None starts a sparse snapshot (possibly paged); Some requires the exact
/// prior overlay revision. Snapshot pages carry offsets and a retained total.
/// Subscription revisions bind messages to the active view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceState {
    pub subscription_revision: u64,
    pub from_revision: Option<u64>,
    pub revision: u64,
    #[serde(deserialize_with = "bounded_changes")]
    pub changes: Vec<ResourceAmount>,
    /// Snapshot page (offset, total); pages share a revision and sorted IDs.
    pub snapshot_page: Option<(u32, u32)>,
}

impl ResourceState {
    pub fn valid(&self) -> bool {
        self.changes.len() <= MAX_RESOURCE_CHANGES
            && self.subscription_revision != 0
            && self.snapshot_page.is_none_or(|(offset, total)| {
                self.from_revision.is_none()
                    && total as usize > MAX_RESOURCE_CHANGES
                    && total as usize <= MAX_RESOURCE_CACHE
                    && !self.changes.is_empty()
                    && u64::from(offset) + self.changes.len() as u64 <= u64::from(total)
            })
            && self
                .from_revision
                .is_none_or(|from| from < self.revision && !self.changes.is_empty())
            && (self.from_revision.is_some()
                || self.revision
                    >= self
                        .changes
                        .iter()
                        .filter(|c| c.id & (1 << 37) == 0)
                        .count() as u64)
            && self.changes.iter().all(|change| {
                change.id & 1 == 0
                    && (change.id >> 37 == 0
                        || (change.id & (1 << 37) != 0 && change.id >> 40 == 0))
            })
            && self.changes.windows(2).all(|pair| pair[0].id < pair[1].id)
    }
}

fn bounded_changes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ResourceAmount>, D::Error> {
    struct Changes;
    impl<'de> Visitor<'de> for Changes {
        type Value = Vec<ResourceAmount>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("at most 65536 resource changes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_RESOURCE_CHANGES)
            {
                return Err(serde::de::Error::custom("too many resource changes"));
            }
            let mut changes = Vec::new();
            while let Some(change) = sequence.next_element()? {
                if changes.len() == MAX_RESOURCE_CHANGES {
                    return Err(serde::de::Error::custom("too many resource changes"));
                }
                changes.push(change);
            }
            Ok(changes)
        }
    }
    deserializer.deserialize_seq(Changes)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests/starting_resources.rs"]
mod starting_tests;
