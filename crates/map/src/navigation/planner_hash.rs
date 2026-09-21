use super::{RoutePlanner, Terminal};
use crate::EnvironmentPageError;

fn environment_error_tag(error: EnvironmentPageError) -> u8 {
    match error {
        EnvironmentPageError::Missing => 0,
        EnvironmentPageError::Corrupt => 1,
        EnvironmentPageError::Unavailable => 2,
        EnvironmentPageError::Cancelled => 3,
        EnvironmentPageError::Invalid => 4,
    }
}

impl RoutePlanner {
    pub fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(&self.origin.x.to_le_bytes());
        hash.update(&self.origin.y.to_le_bytes());
        hash.update(&self.destination.x.to_le_bytes());
        hash.update(&self.destination.y.to_le_bytes());
        hash.update(&self.max_expansions.to_le_bytes());
        hash.update(&self.expansions.to_le_bytes());
        hash.update(&(self.portal_index as u64).to_le_bytes());
        hash.update(&[self.direct_search_started as u8]);
        hash.update(&[self.portals.is_some() as u8]);
        if let Some(portals) = &self.portals {
            hash.update(&(portals.len() as u64).to_le_bytes());
            for tile in portals {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
            }
        }
        hash.update(&[self.search.is_some() as u8]);
        if let Some(search) = &self.search {
            hash.update(&search.goal.x.to_le_bytes());
            hash.update(&search.goal.y.to_le_bytes());
            hash.update(&(search.open.len() as u64).to_le_bytes());
            for node in &search.open {
                hash.update(&node.total.to_le_bytes());
                hash.update(&node.cost.to_le_bytes());
                hash.update(&node.tile.x.to_le_bytes());
                hash.update(&node.tile.y.to_le_bytes());
            }
            hash.update(&(search.scores.len() as u64).to_le_bytes());
            for (tile, score) in &search.scores {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
                hash.update(&score.to_le_bytes());
            }
            hash.update(&(search.parents.len() as u64).to_le_bytes());
            for (tile, parent) in &search.parents {
                hash.update(&tile.x.to_le_bytes());
                hash.update(&tile.y.to_le_bytes());
                hash.update(&parent.x.to_le_bytes());
                hash.update(&parent.y.to_le_bytes());
            }
        }
        match &self.terminal {
            None => {
                hash.update(&[0]);
            }
            Some(Terminal::Path(path)) => {
                hash.update(&[1]);
                hash.update(&path.cost.to_le_bytes());
                hash.update(&(path.tiles.len() as u64).to_le_bytes());
                for tile in &path.tiles {
                    hash.update(&tile.x.to_le_bytes());
                    hash.update(&tile.y.to_le_bytes());
                }
            }
            Some(Terminal::InvalidDestination) => {
                hash.update(&[2]);
            }
            Some(Terminal::Unreachable) => {
                hash.update(&[3]);
            }
            Some(Terminal::SearchLimit) => {
                hash.update(&[4]);
            }
            Some(Terminal::Environment(error)) => {
                hash.update(&[5, environment_error_tag(*error)]);
            }
        }
    }
}
