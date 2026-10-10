//! Query-local lazy five-square halo. No allocation, generator mutation, global
//! state, world array, or speculative source read. Entries die with one point.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct Entry {
    base: Option<Tile>,
    candidate: Option<Option<ResourceNode>>,
}

pub(super) struct PointMemo<'a> {
    generator: &'a MapChunkGenerator,
    center: TileCoord,
    cancelled: &'a dyn Fn() -> bool,
    entries: RefCell<[Entry; 25]>,
}

impl<'a> PointMemo<'a> {
    pub(super) fn new(
        generator: &'a MapChunkGenerator,
        center: TileCoord,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            center,
            cancelled,
            entries: RefCell::new([Entry::default(); 25]),
        }
    }
    fn index(&self, position: TileCoord) -> Option<usize> {
        let x = i64::from(position.x) - i64::from(self.center.x);
        let y = i64::from(position.y) - i64::from(self.center.y);
        ((-2..=2).contains(&x) && (-2..=2).contains(&y)).then_some(((y + 2) * 5 + x + 2) as usize)
    }
    pub(super) fn base(&self, position: TileCoord) -> Result<Option<Tile>, EnvironmentPageError> {
        // Cancellation is observed even on a hit. Do not retain source errors.
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.index(position);
        if let Some(index) = index
            && let Some(base) = self.entries.borrow()[index].base
        {
            return Ok(Some(base));
        }
        let result = self.generator.landscape_base_at(position, self.cancelled)?;
        if let Some(index) = index
            && let Some(base) = result
        {
            self.entries.borrow_mut()[index].base = Some(base);
        }
        Ok(result)
    }
    fn candidate(&self, position: TileCoord) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.index(position);
        if let Some(index) = index
            && let Some(candidate) = self.entries.borrow()[index].candidate
        {
            return Ok(candidate);
        }
        let candidate = self
            .base(position)?
            .and_then(|base| resources::candidate_unreserved(self.generator, position, base));
        if let Some(index) = index {
            self.entries.borrow_mut()[index].candidate = Some(candidate);
        }
        Ok(candidate)
    }
    pub(super) fn reserved(&self, position: TileCoord) -> Result<bool, EnvironmentPageError> {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                if self.candidate(TileCoord::new(x, y))?.is_some() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(super) fn resource(
        &self,
        position: TileCoord,
        base: Tile,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        let Some(node) = self.candidate(position)? else {
            return Ok(None);
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx == 0) == (dy == 0) {
                    continue;
                }
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                let neighbor = TileCoord::new(x, y);
                if let Some(sample) = self.base(neighbor)?
                    && sample.passable
                    && sample.surface.walkable()
                    && (i32::from(base.game_height_level) - i32::from(sample.game_height_level))
                        .abs()
                        <= 1
                    && self.candidate(neighbor)?.is_none()
                {
                    return Ok(Some(node));
                }
            }
        }
        Ok(None)
    }
}

#[path = "memo/tests.rs"]
#[cfg(test)]
mod tests;
