//! Unit id to frame slot, in pages of ids. Ids are allocated in sequence for
//! units, buildings and animals together and never recycled (a unit leaving a
//! building comes back under its own), so units that live on grow sparse in a
//! long match. Memory, by range of `PAGE` ids: 16 bytes of directory, and 1 KiB
//! more only while some unit of the range is in the frame. The directory ends
//! at the highest unit id seen since the frame was last built from nothing.
use super::ABSENT;
use aoe_core::EntityId;

/// Ids per page.
const PAGE: usize = 1 << 8;

#[derive(Default)]
struct Page {
    /// `None` while no id of the page has a slot.
    slots: Option<Box<[u32; PAGE]>>,
    live: u16,
}

#[derive(Default)]
pub(super) struct SlotIndex {
    pages: Vec<Page>,
}

fn place(id: EntityId) -> (usize, usize) {
    (id.0 as usize / PAGE, id.0 as usize % PAGE)
}

impl SlotIndex {
    pub(super) fn get(&self, id: EntityId) -> Option<u32> {
        Some(self[&id]).filter(|slot| *slot != ABSENT)
    }

    pub(super) fn insert(&mut self, id: EntityId, slot: u32) {
        let (page, at) = place(id);
        if page >= self.pages.len() {
            self.pages.resize_with(page + 1, Page::default);
        }
        let page = &mut self.pages[page];
        let slots = page.slots.get_or_insert_with(|| Box::new([ABSENT; PAGE]));
        let old = std::mem::replace(&mut slots[at], slot);
        page.live += u16::from(old == ABSENT);
    }

    pub(super) fn remove(&mut self, id: EntityId) {
        let (page, at) = place(id);
        let Some(page) = self.pages.get_mut(page) else {
            return;
        };
        let Some(slots) = page.slots.as_mut() else {
            return;
        };
        if std::mem::replace(&mut slots[at], ABSENT) == ABSENT {
            return;
        }
        page.live -= 1;
        if page.live == 0 {
            page.slots = None;
        }
    }

    /// Ids that have a slot.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.pages.iter().map(|page| usize::from(page.live)).sum()
    }

    /// Pages that hold slots; an emptied one only keeps its directory entry.
    #[cfg(test)]
    pub(super) fn pages(&self) -> usize {
        self.pages.iter().filter(|p| p.slots.is_some()).count()
    }
}

impl std::ops::Index<&EntityId> for SlotIndex {
    type Output = u32;

    /// `ABSENT` without a slot, which fails the caller's own bounds check.
    fn index(&self, id: &EntityId) -> &u32 {
        let (page, at) = place(*id);
        self.pages
            .get(page)
            .and_then(|page| page.slots.as_ref())
            .map_or(&ABSENT, |slots| &slots[at])
    }
}
