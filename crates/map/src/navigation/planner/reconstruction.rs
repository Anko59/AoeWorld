use super::search;
use super::{MAX_ROUTE_PLANNER_NODES, MAX_ROUTE_TILES, RoutePlanner, Terminal};
use crate::EnvironmentPageError;
use aoe_core::TileCoord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReconstructionPhase {
    Backtrace,
    Convert { next_index: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReconstructionState {
    pub(super) current: TileCoord,
    pub(super) reverse_tiles: Vec<TileCoord>,
    pub(super) phase: ReconstructionPhase,
    pub(super) directions: Vec<u8>,
    pub(super) costs: Vec<u16>,
}

impl ReconstructionState {
    pub(super) fn new(destination: TileCoord) -> Self {
        Self {
            current: destination,
            reverse_tiles: vec![destination],
            phase: ReconstructionPhase::Backtrace,
            directions: Vec::new(),
            costs: Vec::new(),
        }
    }
}

impl RoutePlanner {
    pub(super) fn reconstruction_has_free_step(&self) -> bool {
        self.reconstruction
            .as_ref()
            .is_some_and(|reconstruction| match reconstruction.phase {
                ReconstructionPhase::Backtrace => reconstruction.current == self.origin,
                ReconstructionPhase::Convert { next_index } => next_index == 0,
            })
    }

    pub(super) fn advance_reconstruction(
        &mut self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        if cancelled() {
            return Err(Terminal::Environment(EnvironmentPageError::Cancelled));
        }
        let Some(phase) = self.reconstruction.as_ref().map(|state| state.phase) else {
            return Err(Terminal::SearchLimit);
        };
        match phase {
            ReconstructionPhase::Backtrace => {
                let Some(current) = self.reconstruction.as_ref().map(|state| state.current) else {
                    return Err(Terminal::Unreachable);
                };
                if current == self.origin {
                    let Some(next_index) = self
                        .reconstruction
                        .as_ref()
                        .map(|state| state.reverse_tiles.len().saturating_sub(1))
                    else {
                        return Err(Terminal::Unreachable);
                    };
                    if let Some(reconstruction) = self.reconstruction.as_mut() {
                        reconstruction.phase = ReconstructionPhase::Convert { next_index };
                    }
                    if next_index == 0 {
                        self.complete_reconstruction()?;
                    }
                    return Ok(());
                }
                let Some(parent) = self
                    .search
                    .as_ref()
                    .and_then(|search| search.records.get(&current))
                    .and_then(|record| record.parent)
                else {
                    return Err(Terminal::Unreachable);
                };
                let Some(reverse_len) = self
                    .reconstruction
                    .as_ref()
                    .map(|reconstruction| reconstruction.reverse_tiles.len())
                else {
                    return Err(Terminal::Unreachable);
                };
                if reverse_len >= MAX_ROUTE_TILES
                    || self.retained_entries().saturating_add(1) > MAX_ROUTE_PLANNER_NODES
                {
                    return Err(Terminal::SearchLimit);
                }
                let Some(reconstruction) = self.reconstruction.as_mut() else {
                    return Err(Terminal::Unreachable);
                };
                reconstruction.reverse_tiles.push(parent);
                reconstruction.current = parent;
                self.work = self.work.saturating_add(1);
            }
            ReconstructionPhase::Convert { next_index } => {
                if next_index == 0 {
                    self.complete_reconstruction()?;
                    return Ok(());
                }
                let Some(reconstruction) = self.reconstruction.as_ref() else {
                    return Err(Terminal::Unreachable);
                };
                let before = reconstruction.reverse_tiles[next_index];
                let after = reconstruction.reverse_tiles[next_index - 1];
                let dx = after.x - before.x;
                let dy = after.y - before.y;
                let direction = search::direction_index(dx, dy).ok_or(Terminal::Unreachable)?;
                let Some(search) = self.search.as_ref() else {
                    return Err(Terminal::Unreachable);
                };
                let before_cost = search
                    .records
                    .get(&before)
                    .map(|record| record.cost)
                    .ok_or(Terminal::Unreachable)?;
                let after_cost = search
                    .records
                    .get(&after)
                    .map(|record| record.cost)
                    .ok_or(Terminal::Unreachable)?;
                let step_cost = u16::try_from(after_cost.saturating_sub(before_cost))
                    .map_err(|_| Terminal::Unreachable)?;
                if self.retained_entries().saturating_add(2) > MAX_ROUTE_PLANNER_NODES {
                    return Err(Terminal::SearchLimit);
                }
                let Some(reconstruction) = self.reconstruction.as_mut() else {
                    return Err(Terminal::Unreachable);
                };
                reconstruction.directions.push(direction);
                reconstruction.costs.push(step_cost);
                reconstruction.phase = ReconstructionPhase::Convert {
                    next_index: next_index - 1,
                };
                self.work = self.work.saturating_add(1);
                self.update_peak_retained_entries();
                if next_index == 1 {
                    self.complete_reconstruction()?;
                }
            }
        }
        self.update_peak_retained_entries();
        if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
            return Err(Terminal::SearchLimit);
        }
        Ok(())
    }

    fn complete_reconstruction(&mut self) -> Result<(), Terminal> {
        let Some(reconstruction) = self.reconstruction.take() else {
            return Err(Terminal::Unreachable);
        };
        self.route_directions = Some(reconstruction.directions);
        self.route_costs = reconstruction.costs;
        self.route_cursor = 0;
        self.route_position = self.origin;
        self.search = None;
        self.update_peak_retained_entries();
        if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
            return Err(Terminal::SearchLimit);
        }
        Ok(())
    }
}
