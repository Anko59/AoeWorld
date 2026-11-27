//! Source membership and refresh phases for one branch of management.
use super::{Walking, ZoneCache};
use crate::{GameWorld, OfficeZone, OfficeZoneChunk};
use aoe_core::{EntityId, PlayerId, REFERENCE_HZ, TileRect};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

type Key = (u32, PlayerId);
type Shapes = BTreeMap<EntityId, Arc<[OfficeZoneChunk]>>;

/// Game seconds between two looks at one player's zones. Any felled tree on
/// the map makes every zone stale, so on the owner's Anatolia 4 (99 AIs) each
/// player rebuilt its Steward and Vassal zones every second: 66M of 640M
/// main-thread instructions a tick (#978). Ownership lagging a new wall or a
/// felled tree by a few seconds changes nothing a Steward does; a new or
/// removed office, or a bridge, is still seen within a second.
const REFRESH_SECONDS: u64 = 8;

#[derive(Debug, Default)]
pub(crate) struct Groups(BTreeMap<Key, Group>);

#[derive(Debug, Default)]
struct Group {
    sources: BTreeMap<EntityId, TileRect>,
    walking: Option<Arc<Walking>>,
    /// Only groups rendered by a browser allocate a shape cache.
    shapes: OnceLock<Box<Mutex<Shapes>>>,
    membership_changed: bool,
    /// A bridge changed the ground: looked at within a second, not a period;
    /// cleared by that look.
    ground_changed: bool,
}

impl Groups {
    /// Whether the player's offices changed: their published zones are then
    /// stale until the group's next look, within a second.
    pub(in super::super::super) fn set(
        &mut self,
        player: PlayerId,
        office: EntityId,
        source: Option<TileRect>,
    ) -> bool {
        let key = Self::key(player);
        let mut changed = false;
        if let Some(rect) = source {
            let group = self.0.entry(key).or_default();
            changed = group.sources.insert(office, rect) != Some(rect);
            group.membership_changed |= changed;
        } else if let Some(group) = self.0.get_mut(&key) {
            changed = group.sources.remove(&office).is_some();
            group.membership_changed |= changed;
            if group.sources.is_empty() {
                self.0.remove(&key);
            }
        }
        changed
    }

    pub(super) fn shapes(&self, viewer: PlayerId) -> impl Iterator<Item = OfficeZone> + '_ {
        self.0
            .get(&Self::key(viewer))
            .and_then(|group| Some((group, group.walking.as_deref()?)))
            .into_iter()
            .flat_map(|(group, walking)| {
                group.sources.keys().filter_map(move |&id| {
                    let index = walking.index(id)?;
                    let mut shapes = group
                        .shapes
                        .get_or_init(Default::default)
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let shape = shapes
                        .entry(id)
                        .or_insert_with(|| walking.build_shape(index));
                    Some((id, shape.clone()))
                })
            })
    }

    pub(super) fn key(player: PlayerId) -> Key {
        // Leave the economy's boundary tick to the existing defence pass.
        (1 + u32::from(player.0) % (REFERENCE_HZ - 1), player)
    }

    /// Every group looks again on its next turn of the second.
    pub(super) fn ground_changed(&mut self) {
        for group in self.0.values_mut() {
            group.ground_changed = true;
        }
    }

    /// The groups to look at this tick. A bridge's look is taken here, so a
    /// group it left current returns to its period.
    pub(in super::super::super) fn due(&mut self, tick: u64, tick_hz: u32) -> Vec<Key> {
        let hz = u64::from(tick_hz);
        let phase = tick % hz;
        let bucket = |offset: u64| ((offset * u64::from(REFERENCE_HZ)).div_ceil(hz)) as u32;
        // A whole game second always services every reference bucket.
        // Tick one also covers the phase at world creation.
        let first = if tick == 1 { 0 } else { bucket(phase) };
        let turn = tick / hz % REFRESH_SECONDS;
        self.0
            .range_mut((first, PlayerId(0))..(bucket(phase + 1), PlayerId(0)))
            .filter_map(|(&key, group)| {
                let bridge = std::mem::take(&mut group.ground_changed);
                (tick == 1
                    || bridge
                    || group.membership_changed
                    || u64::from(key.1.0) % REFRESH_SECONDS == turn)
                    .then_some(key)
            })
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
            if let Some(shapes) = group.shapes.get_mut() {
                shapes
                    .get_mut()
                    .unwrap_or_else(|error| error.into_inner())
                    .retain(|id, _| group.sources.contains_key(id) && !walking.zone_changed(*id));
            }
            group.walking = Some(walking);
            group.membership_changed = false;
        }
    }
}
