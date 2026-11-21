//! Source membership and refresh phases for one branch of management.
use super::{Walking, ZoneCache};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, REFERENCE_HZ, TileRect};
use std::collections::BTreeMap;
use std::sync::Arc;

type Key = (u32, PlayerId);

#[derive(Debug, Default)]
pub(crate) struct Groups(BTreeMap<Key, Group>);

#[derive(Debug, Default)]
struct Group {
    sources: BTreeMap<EntityId, TileRect>,
    walking: Option<Arc<Walking>>,
    membership_changed: bool,
}

impl Groups {
    pub(in super::super::super) fn set(
        &mut self,
        player: PlayerId,
        office: EntityId,
        source: Option<TileRect>,
    ) {
        let key = Self::key(player);
        if let Some(rect) = source {
            let group = self.0.entry(key).or_default();
            group.membership_changed |= group.sources.insert(office, rect) != Some(rect);
        } else if let Some(group) = self.0.get_mut(&key) {
            group.membership_changed |= group.sources.remove(&office).is_some();
            if group.sources.is_empty() {
                self.0.remove(&key);
            }
        }
    }

    pub(super) fn key(player: PlayerId) -> Key {
        // Leave the economy's boundary tick to the existing defence pass.
        (1 + u32::from(player.0) % (REFERENCE_HZ - 1), player)
    }

    pub(in super::super::super) fn due(&self, tick: u64, tick_hz: u32) -> Vec<Key> {
        let hz = u64::from(tick_hz);
        let phase = tick % hz;
        let bucket = |offset: u64| ((offset * u64::from(REFERENCE_HZ)).div_ceil(hz)) as u32;
        // A whole game second always services every reference bucket.
        // Tick one also covers the phase at world creation.
        let first = if tick == 1 { 0 } else { bucket(phase) };
        self.0
            .range((first, PlayerId(0))..(bucket(phase + 1), PlayerId(0)))
            .map(|(&key, _)| key)
            .collect()
    }

    pub(in super::super::super) fn refresh(
        &self,
        world: &GameWorld,
        key: Key,
        cache: &ZoneCache,
    ) -> Option<Arc<Walking>> {
        let group = self.0.get(&key)?;
        let old = group.walking.as_ref();
        if !group.membership_changed && old.is_some_and(|walking| walking.current(cache)) {
            return None;
        }
        let sources = group
            .sources
            .iter()
            .map(|(&id, &rect)| (id, rect))
            .collect();
        Walking::new(world, key.1, sources, old).map(Arc::new)
    }

    pub(in super::super::super) fn publish(&mut self, key: Key, walking: Arc<Walking>) {
        if let Some(group) = self.0.get_mut(&key) {
            group.walking = Some(walking);
            group.membership_changed = false;
        }
    }
}
