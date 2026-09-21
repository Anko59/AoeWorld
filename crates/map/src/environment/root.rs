use crate::EnvironmentError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PageLayer {
    Elevation,
    Water,
    Vegetation,
    HistoricalLandUse,
}

impl PageLayer {
    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Elevation => "elevation",
            Self::Water => "water",
            Self::Vegetation => "vegetation",
            Self::HistoricalLandUse => "historical-land-use",
        }
    }

    fn domain(self) -> &'static [u8] {
        match self {
            Self::Elevation => b"aoe-environment-page-root-v1\0",
            Self::Water => b"aoe-water-page-root-v1\0",
            Self::Vegetation => b"aoe-potential-biome-page-root-v1\0",
            Self::HistoricalLandUse => b"aoe-historical-land-use-page-root-v1\0",
        }
    }
}

/// Streams the canonical hashes for one pyramid level. The adapter must
/// validate each page's coordinates and append it in canonical (y, x) order.
/// Storage is constant regardless of how many pages belong to the level.
pub struct PageRootBuilder {
    hash: blake3::Hasher,
    expected: usize,
    appended: usize,
}

impl PageRootBuilder {
    pub fn new(layer: PageLayer, expected: usize) -> Result<Self, EnvironmentError> {
        if expected == 0 {
            return Err(EnvironmentError::InvalidPyramid);
        }
        let mut hash = blake3::Hasher::new();
        hash.update(layer.domain());
        hash.update(&(expected as u64).to_le_bytes());
        Ok(Self {
            hash,
            expected,
            appended: 0,
        })
    }

    pub fn push(&mut self, content_hash: [u8; 32]) -> Result<(), EnvironmentError> {
        if self.appended == self.expected {
            return Err(EnvironmentError::InvalidPyramid);
        }
        self.hash.update(&content_hash);
        self.appended += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<[u8; 32], EnvironmentError> {
        if self.appended != self.expected {
            return Err(EnvironmentError::InvalidPyramid);
        }
        Ok(*self.hash.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_requires_exact_count_and_retains_layer_and_order_identity() {
        let build = |layer, hashes: [[u8; 32]; 2]| {
            let mut root = PageRootBuilder::new(layer, 2).expect("root");
            for hash in hashes {
                root.push(hash).expect("page");
            }
            root.finish().expect("complete")
        };
        let original = build(PageLayer::Elevation, [[1; 32], [2; 32]]);
        assert_ne!(original, build(PageLayer::Water, [[1; 32], [2; 32]]));
        assert_ne!(original, build(PageLayer::Elevation, [[2; 32], [1; 32]]));
        let mut root = PageRootBuilder::new(PageLayer::Elevation, 1).expect("root");
        root.push([1; 32]).expect("page");
        assert_eq!(root.push([2; 32]), Err(EnvironmentError::InvalidPyramid));
        assert_eq!(
            PageRootBuilder::new(PageLayer::Water, 1)
                .expect("root")
                .finish(),
            Err(EnvironmentError::InvalidPyramid)
        );
        assert!(PageRootBuilder::new(PageLayer::Water, 0).is_err());
    }
}
