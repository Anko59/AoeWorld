use super::{MAX_RESOURCE_OVERLAY_CHANGES, ResourceOverlay, ResourceOverlayError};
use crate::{EnvironmentPageError, MapChunkGenerator};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{collections::BTreeMap, fmt};

pub const RESOURCE_OVERLAY_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceChange {
    pub id: u64,
    pub remaining: u16,
}

/// Mutable state is bound to one immutable package, never just to its seed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceOverlaySnapshot {
    pub schema_version: u16,
    pub map_content_hash: [u8; 32],
    pub revision: u64,
    #[serde(deserialize_with = "bounded_changes")]
    pub changes: Vec<ResourceChange>,
}

impl ResourceOverlay {
    pub fn snapshot(&self, map_content_hash: [u8; 32]) -> ResourceOverlaySnapshot {
        ResourceOverlaySnapshot {
            schema_version: RESOURCE_OVERLAY_SCHEMA_VERSION,
            map_content_hash,
            revision: self.revision,
            changes: self
                .changed()
                .map(|(id, remaining)| ResourceChange { id, remaining })
                .collect(),
        }
    }

    /// Validates the complete snapshot before returning replacement state.
    /// Callers retain their existing overlay when validation or I/O fails.
    pub fn from_snapshot(
        snapshot: &ResourceOverlaySnapshot,
        expected_map_content_hash: [u8; 32],
        terrain: &MapChunkGenerator,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, ResourceOverlayError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled.into());
        }
        if snapshot.map_content_hash != expected_map_content_hash {
            return Err(ResourceOverlayError::WrongMap);
        }
        if snapshot.schema_version != RESOURCE_OVERLAY_SCHEMA_VERSION
            || snapshot.changes.len() > MAX_RESOURCE_OVERLAY_CHANGES
            || snapshot.revision < snapshot.changes.len() as u64
        {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        let mut remaining = BTreeMap::new();
        let mut previous = None;
        let mut removed_total = 0_u64;
        for change in &snapshot.changes {
            if previous.is_some_and(|id| id >= change.id) {
                return Err(ResourceOverlayError::InvalidSnapshot);
            }
            let node = terrain
                .resource_by_id_with_cancel(change.id, cancelled)?
                .ok_or(ResourceOverlayError::UnknownResource)?;
            if change.remaining >= node.initial_amount {
                return Err(ResourceOverlayError::InvalidSnapshot);
            }
            removed_total += u64::from(node.initial_amount - change.remaining);
            remaining.insert(change.id, change.remaining);
            previous = Some(change.id);
        }
        // Each changing depletion removes at least one unit and changes one
        // entry; zero-removal calls never increase the revision.
        if snapshot.revision > removed_total {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        Ok(Self {
            remaining,
            revision: snapshot.revision,
        })
    }
}

fn bounded_changes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ResourceChange>, D::Error> {
    struct Changes;
    impl<'de> Visitor<'de> for Changes {
        type Value = Vec<ResourceChange>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded sequence of resource changes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_RESOURCE_OVERLAY_CHANGES)
            {
                return Err(serde::de::Error::custom("too many resource changes"));
            }
            let mut changes = Vec::with_capacity(
                sequence
                    .size_hint()
                    .unwrap_or(0)
                    .min(MAX_RESOURCE_OVERLAY_CHANGES),
            );
            while let Some(change) = sequence.next_element::<ResourceChange>()? {
                if changes.len() == MAX_RESOURCE_OVERLAY_CHANGES {
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
