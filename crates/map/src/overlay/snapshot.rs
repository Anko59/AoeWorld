use super::{MAX_RESOURCE_OVERLAY_CHANGES, ResourceOverlay, ResourceOverlayError};
use crate::{EnvironmentPageError, MapChunkGenerator, ResourceNode};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{SeqAccess, Visitor},
};
use std::{collections::BTreeMap, fmt};

pub const RESOURCE_OVERLAY_SCHEMA_VERSION: u16 = 3;

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
    pub replenished: u64,
    #[serde(deserialize_with = "bounded_sequence")]
    pub changes: Vec<ResourceChange>,
    /// Clock used for the remaining ticks in recovery.
    pub recovery_hz: u32,
    /// Pending recovery as (remaining ticks, resource ID), in ascending order.
    #[serde(deserialize_with = "bounded_sequence")]
    pub recovery: Vec<(u64, u64)>,
}

impl ResourceOverlay {
    pub fn snapshot(&self, map_content_hash: [u8; 32]) -> ResourceOverlaySnapshot {
        ResourceOverlaySnapshot {
            schema_version: RESOURCE_OVERLAY_SCHEMA_VERSION,
            map_content_hash,
            revision: self.revision,
            replenished: self.replenished,
            recovery_hz: aoe_core::REFERENCE_HZ,
            recovery: Vec::new(),
            changes: self
                .changed()
                .map(|(id, remaining)| ResourceChange { id, remaining })
                .collect(),
        }
    }

    /// Validates the resource amounts before returning replacement terrain state.
    /// Simulation restores its recovery schedule from the same snapshot.
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
        {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        let mut remaining = BTreeMap::new();
        let mut starting = Self::default();
        let mut depleted_count = 0;
        let mut previous = None;
        let mut net_removed = 0_u64;
        let mut fully_regrown = 0_u64;
        for change in &snapshot.changes {
            if previous.is_some_and(|id| id >= change.id) {
                return Err(ResourceOverlayError::InvalidSnapshot);
            }
            let added = ResourceNode::starting_from_id(change.id);
            if let Some(node) = added {
                starting.insert_starting_resource(terrain, node)?;
            }
            let node = added
                .or(terrain.resource_by_id_with_cancel(change.id, cancelled)?)
                .ok_or(ResourceOverlayError::UnknownResource)?;
            if change.remaining > node.initial_amount {
                return Err(ResourceOverlayError::InvalidSnapshot);
            }
            if added.is_none() && change.remaining == node.initial_amount {
                fully_regrown += 1;
            }
            depleted_count += u64::from(change.remaining < node.initial_amount);
            net_removed += u64::from(node.initial_amount - change.remaining);
            remaining.insert(change.id, change.remaining);
            previous = Some(change.id);
        }
        // Every operation changes at least one unit. Total harvested stock is
        // net removal plus stock restored since the beginning of the match.
        let maximum_revision = net_removed.saturating_add(snapshot.replenished.saturating_mul(2));
        if snapshot.revision > maximum_revision
            || snapshot.revision < depleted_count + 2 * fully_regrown
            || snapshot.replenished < fully_regrown
        {
            return Err(ResourceOverlayError::InvalidSnapshot);
        }
        let mut overlay = Self {
            remaining,
            revision: snapshot.revision,
            replenished: snapshot.replenished,
            ..Self::default()
        };
        overlay.rebuild_navigation();
        Ok(overlay)
    }
}

fn bounded_sequence<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Changes<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Changes<T> {
        type Value = Vec<T>;
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
            while let Some(change) = sequence.next_element::<T>()? {
                if changes.len() == MAX_RESOURCE_OVERLAY_CHANGES {
                    return Err(serde::de::Error::custom("too many resource changes"));
                }
                changes.push(change);
            }
            Ok(changes)
        }
    }
    deserializer.deserialize_seq(Changes(std::marker::PhantomData))
}

#[cfg(test)]
mod tests;
