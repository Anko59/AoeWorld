use super::packed::PackedChunk;
use super::*;

#[derive(Debug)]
pub(crate) struct ChunkPortals {
    pub(super) portals: Vec<Portal>,
    pub(super) costs: OnceLock<Vec<Vec<Option<u32>>>>,
    pub(super) labels: grid::Labels,
    pub(super) components: u16,
    pub(super) links: Vec<(u16, u16)>,
    pub(super) packed: Option<PackedChunk>,
}

impl ChunkPortals {
    pub(super) fn from_packed(packed: PackedChunk) -> Self {
        Self {
            portals: Vec::new(),
            costs: OnceLock::new(),
            labels: grid::Labels::Empty,
            components: 0,
            links: Vec::new(),
            packed: Some(packed),
        }
    }
    pub(super) fn portal_count(&self) -> usize {
        self.packed
            .as_ref()
            .map_or(self.portals.len(), |p| p.view().portal_count())
    }
    pub(super) fn portals(&self) -> impl ExactSizeIterator<Item = Portal> + '_ {
        let view = self.packed.as_ref().map(PackedChunk::view);
        (0..self.portal_count()).map(move |at| match view {
            Some(p) => p.portal(at),
            None => self.portals[at],
        })
    }
    pub(super) fn label(&self, at: usize) -> u16 {
        match &self.packed {
            Some(p) => p.view().label(at),
            None => self.labels[at],
        }
    }
    pub(super) fn labels(&self) -> impl Iterator<Item = u16> + '_ {
        let view = self.packed.as_ref().map(PackedChunk::view);
        self.labels
            .iter()
            .enumerate()
            .map(move |(at, label)| match view {
                Some(p) => p.label(at),
                None => *label,
            })
    }
    pub(super) fn components(&self) -> u16 {
        self.packed
            .as_ref()
            .map_or(self.components, |p| p.view().components())
    }
    pub(super) fn links(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        let view = self.packed.as_ref().map(PackedChunk::view);
        let count = view.map_or(self.links.len(), |p| p.link_count());
        (0..count).map(move |at| match view {
            Some(p) => p.link(at),
            None => self.links[at],
        })
    }
    pub(super) fn cost(&self, from: usize, to: usize) -> Option<u32> {
        match &self.packed {
            Some(p) => p.view().cost(from, to),
            None => self.costs.get()?.get(from)?.get(to).copied().flatten(),
        }
    }
    pub(super) fn bytes(&self) -> usize {
        let count = self.portals.capacity();
        size_of::<Self>()
            + count * size_of::<Portal>()
            + count * count * size_of::<Option<u32>>()
            + self.labels.bytes()
            + self.links.capacity() * size_of::<(u16, u16)>()
            + count * size_of::<Vec<Option<u32>>>()
            + 64
    }
}
