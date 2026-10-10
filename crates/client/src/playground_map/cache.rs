//! One owned decoded cache with explicit transport provenance.
use super::*;
use aoe_core::TileCoord;
use aoe_map::{
    LandscapeAppearance, LandscapeChunk, LandscapeChunkError, LandscapeDecoration,
    LandscapeResource, LandscapeTile,
};

#[derive(Clone, Debug)]
pub(crate) enum CachedChunk {
    Legacy(Chunk),
    Landscape(LandscapeChunk),
}

impl CachedChunk {
    pub(crate) fn decode(compact: &CompactChunk) -> Result<Self, LandscapeChunkError> {
        if compact.payload_hex.starts_with("03") {
            compact.decode_landscape_v3().map(Self::Landscape)
        } else {
            compact.decode().map(Self::Legacy).map_err(Into::into)
        }
    }

    pub(crate) fn coordinate(&self) -> (i32, i32) {
        match self {
            Self::Legacy(chunk) => (chunk.x, chunk.y),
            Self::Landscape(chunk) => (chunk.x, chunk.y),
        }
    }

    fn slices(&self) -> (&[Tile], &[LandscapeTile]) {
        match self {
            Self::Legacy(chunk) => (&chunk.tiles, &[]),
            Self::Landscape(chunk) => (&[], &chunk.tiles),
        }
    }

    pub(crate) fn base_tiles(&self) -> impl Iterator<Item = &Tile> {
        let mut index = 0;
        std::iter::from_fn(move || {
            let tile = self.base_tile_at(index)?;
            index += 1;
            Some(tile)
        })
    }

    // Share the height-fold loop across legacy and explicit-coordinate records.
    fn base_tile_at(&self, index: usize) -> Option<&Tile> {
        match self {
            Self::Legacy(chunk) => chunk.tiles.get(index),
            Self::Landscape(chunk) => chunk.tiles.get(index).map(|tile| &tile.terrain),
        }
    }

    pub(crate) fn scene_tiles(
        &self,
        width: i32,
        height: i32,
    ) -> impl Iterator<Item = (TileCoord, &Tile, Option<LandscapeAppearance>)> {
        let (cx, cy) = self.coordinate();
        let columns = chunk_axis_len(width, cx);
        let rows = chunk_axis_len(height, cy);
        let (legacy, landscape) = self.slices();
        legacy
            .iter()
            .enumerate()
            .filter_map(move |(index, tile)| {
                if columns == 0 || index / columns >= rows {
                    return None;
                }
                Some((
                    TileCoord::new(
                        cx * CHUNK_TILES + (index % columns) as i32,
                        cy * CHUNK_TILES + (index / columns) as i32,
                    ),
                    tile,
                    None,
                ))
            })
            .chain(
                landscape
                    .iter()
                    .map(|tile| (tile.tile, &tile.terrain, tile.appearance)),
            )
    }

    pub(crate) fn tile_at(&self, width: i32, height: i32, x: i32, y: i32) -> Option<&Tile> {
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        match self {
            Self::Legacy(chunk) => chunk_tile_index(width, height, chunk, x, y)
                .and_then(|index| chunk.tiles.get(index)),
            Self::Landscape(chunk) => chunk
                .tiles
                .binary_search_by_key(&(y, x), |tile| (tile.tile.y, tile.tile.x))
                .ok()
                .and_then(|index| chunk.tiles.get(index))
                .map(|tile| &tile.terrain),
        }
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = (&ResourceNode, u8)> {
        let mut index = 0;
        std::iter::from_fn(move || {
            let resource = self.resource_at(index)?;
            index += 1;
            Some(resource)
        })
    }

    fn resource_at(&self, index: usize) -> Option<(&ResourceNode, u8)> {
        match self {
            Self::Legacy(chunk) => chunk.resources.get(index).map(|resource| (resource, 0)),
            Self::Landscape(chunk) => chunk
                .resources
                .get(index)
                .map(|resource| (&resource.node, resource.visual_family as u8)),
        }
    }

    pub(crate) fn decorations(&self) -> &[LandscapeDecoration] {
        match self {
            Self::Legacy(_) => &[],
            Self::Landscape(chunk) => &chunk.decorations,
        }
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        let owned = match self {
            Self::Legacy(chunk) => chunk
                .tiles
                .capacity()
                .saturating_mul(size_of::<Tile>())
                .saturating_add(
                    chunk
                        .resources
                        .capacity()
                        .saturating_mul(size_of::<ResourceNode>()),
                ),
            Self::Landscape(chunk) => chunk
                .tiles
                .capacity()
                .saturating_mul(size_of::<LandscapeTile>())
                .saturating_add(
                    chunk
                        .resources
                        .capacity()
                        .saturating_mul(size_of::<LandscapeResource>()),
                )
                .saturating_add(
                    chunk
                        .decorations
                        .capacity()
                        .saturating_mul(size_of::<LandscapeDecoration>()),
                ),
        };
        size_of::<Self>().saturating_add(owned)
    }
}

impl From<Chunk> for CachedChunk {
    fn from(chunk: Chunk) -> Self {
        Self::Legacy(chunk)
    }
}

#[path = "cache/tests.rs"]
#[cfg(test)]
mod tests;
