use super::{ReconstructionPhase, ReconstructionState, RoutePlanner, SearchState, Terminal};
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
        hash.update(&self.max_work.to_le_bytes());
        hash.update(&self.work.to_le_bytes());
        hash.update(&self.expansions.to_le_bytes());
        hash.update(&[self.validation_step]);
        hash_usize(hash, self.route_cursor);
        hash.update(&self.route_position.x.to_le_bytes());
        hash.update(&self.route_position.y.to_le_bytes());
        hash.update(&(self.peak_retained_entries as u64).to_le_bytes());
        hash.update(&[self.search.is_some() as u8]);
        if let Some(search) = &self.search {
            hash_search(search, hash);
        }
        hash.update(&[self.reconstruction.is_some() as u8]);
        if let Some(reconstruction) = &self.reconstruction {
            hash_reconstruction(reconstruction, hash);
        }
        hash.update(&[self.route_directions.is_some() as u8]);
        if let Some(route) = &self.route_directions {
            hash.update(&(route.len() as u64).to_le_bytes());
            hash.update(route);
        }
        hash.update(&(self.route_costs.len() as u64).to_le_bytes());
        for cost in &self.route_costs {
            hash.update(&cost.to_le_bytes());
        }
        match self.terminal {
            None => hash.update(&[0]),
            Some(Terminal::Complete) => hash.update(&[1]),
            Some(Terminal::InvalidDestination) => hash.update(&[2]),
            Some(Terminal::Unreachable) => hash.update(&[3]),
            Some(Terminal::SearchLimit) => hash.update(&[4]),
            Some(Terminal::Environment(error)) => hash.update(&[5, environment_error_tag(error)]),
        };
    }
}

fn hash_usize(hash: &mut blake3::Hasher, value: usize) {
    hash.update(&(value as u64).to_le_bytes());
}

fn hash_reconstruction(reconstruction: &ReconstructionState, hash: &mut blake3::Hasher) {
    hash.update(&reconstruction.current.x.to_le_bytes());
    hash.update(&reconstruction.current.y.to_le_bytes());
    hash.update(&(reconstruction.reverse_tiles.len() as u64).to_le_bytes());
    for tile in &reconstruction.reverse_tiles {
        hash.update(&tile.x.to_le_bytes());
        hash.update(&tile.y.to_le_bytes());
    }
    match reconstruction.phase {
        ReconstructionPhase::Backtrace => {
            hash.update(&[0]);
        }
        ReconstructionPhase::Convert { next_index } => {
            hash.update(&[1]);
            hash_usize(hash, next_index);
        }
    };
    hash.update(&(reconstruction.directions.len() as u64).to_le_bytes());
    hash.update(&reconstruction.directions);
    hash.update(&(reconstruction.costs.len() as u64).to_le_bytes());
    for cost in &reconstruction.costs {
        hash.update(&cost.to_le_bytes());
    }
}

fn hash_search(search: &SearchState, hash: &mut blake3::Hasher) {
    hash.update(&(search.open.len() as u64).to_le_bytes());
    for node in &search.open {
        hash.update(&node.total.to_le_bytes());
        hash.update(&node.cost.to_le_bytes());
        hash.update(&node.tile.x.to_le_bytes());
        hash.update(&node.tile.y.to_le_bytes());
    }
    hash.update(&(search.records.len() as u64).to_le_bytes());
    for (tile, record) in &search.records {
        hash.update(&tile.x.to_le_bytes());
        hash.update(&tile.y.to_le_bytes());
        hash.update(&record.cost.to_le_bytes());
        match record.parent {
            None => {
                hash.update(&[0]);
            }
            Some(parent) => {
                hash.update(&[1]);
                hash.update(&parent.x.to_le_bytes());
                hash.update(&parent.y.to_le_bytes());
            }
        };
    }
    match search.active {
        None => {
            hash.update(&[0]);
        }
        Some(active) => {
            hash.update(&[1, active.next_neighbor]);
            hash.update(&active.cost.to_le_bytes());
            hash.update(&active.tile.x.to_le_bytes());
            hash.update(&active.tile.y.to_le_bytes());
        }
    };
}

#[cfg(test)]
mod tests {
    use super::hash_usize;

    #[test]
    fn usize_hash_encoding_has_a_fixed_u64_width() {
        let mut actual = blake3::Hasher::new();
        hash_usize(&mut actual, 0x0102_0304);
        assert_eq!(actual.finalize(), blake3::hash(&[4, 3, 2, 1, 0, 0, 0, 0]));
    }
}
