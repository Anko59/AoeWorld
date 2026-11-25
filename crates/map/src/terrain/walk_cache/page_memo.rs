//! Recent provider pages of one navigation or raster worker.
use std::sync::Arc;

const PAGE_MEMO_PAGES: usize = 32;

/// The last `PAGE_MEMO_PAGES` immutable pages one generator loaded through its
/// provider, replaced first in, first out: a navigation worker sweeps chunks,
/// and a raster worker blocks, in order, so the oldest load is the one it is
/// done with. A raster sizes its blocks to fit (the server's `minimap_blocks`).
#[derive(Debug, Default)]
pub(in crate::terrain) struct PageMemo {
    entries: Vec<(crate::EnvironmentPageKey, Arc<crate::EnvironmentPage>)>,
    next: usize,
}

impl PageMemo {
    pub(in crate::terrain) fn get(
        &self,
        key: crate::EnvironmentPageKey,
    ) -> Option<Arc<crate::EnvironmentPage>> {
        self.entries
            .iter()
            .find(|(held, _)| *held == key)
            .map(|(_, page)| Arc::clone(page))
    }

    pub(in crate::terrain) fn insert(
        &mut self,
        key: crate::EnvironmentPageKey,
        page: Arc<crate::EnvironmentPage>,
    ) {
        if self.entries.len() < PAGE_MEMO_PAGES {
            self.entries.push((key, page));
        } else {
            self.entries[self.next] = (key, page);
            self.next = (self.next + 1) % PAGE_MEMO_PAGES;
        }
    }
}
