//! Compact native actor tags shared by scene selection and all renderers.
use super::SceneUnitKind;
#[path = "scene_types/actor_map.rs"]
mod mapping;
impl SceneUnitKind {
    pub fn actor(index: u8) -> Self {
        mapping::ACTORS
            .get(usize::from(index))
            .copied()
            .unwrap_or(Self::Scout)
    }
    pub(in crate::game_renderer) fn native_directions(self) -> bool {
        self.actor_slot()
            .is_some_and(|slot| !matches!(slot, 72 | 73 | 120))
    }
    pub(in crate::game_renderer) fn actor_slot(self) -> Option<usize> {
        (self as usize)
            .checked_sub(mapping::BASE)
            .and_then(|i| mapping::SLOTS.get(i).copied())
    }
}
