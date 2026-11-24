//! The memo as the landscape evaluation's per-tile cache.
use super::*;

impl<const N: usize, const W: usize> super::super::LandscapeCache for Memo<'_, N, W> {
    fn base(&self, tile: TileCoord) -> Result<Tile, EnvironmentPageError> {
        Memo::base(self, tile)?.ok_or(EnvironmentPageError::Invalid)
    }
    fn wildness(&self, cx: i64, cy: i64) -> Result<u16, EnvironmentPageError> {
        Memo::wildness(self, cx, cy)
    }
    fn input(&self, tile: TileCoord) -> Option<crate::landscape_patches::Input> {
        let index = self.index(tile)?;
        self.entries.borrow()[index].input
    }
    fn set_input(&self, tile: TileCoord, input: crate::landscape_patches::Input) {
        if let Some(index) = self.index(tile) {
            self.put(index, tile, |entry| entry.input = Some(input));
        }
    }
    fn density(
        &self,
        tile: TileCoord,
        explicit: Option<u16>,
    ) -> Option<crate::landscape_patches::DensitySample> {
        let index = self.index(tile)?;
        let (key, density) = self.entries.borrow()[index].density?;
        (key == explicit).then_some(density)
    }
    fn set_density(
        &self,
        tile: TileCoord,
        explicit: Option<u16>,
        density: crate::landscape_patches::DensitySample,
    ) {
        if let Some(index) = self.index(tile) {
            self.put(index, tile, |entry| {
                entry.density = Some((explicit, density))
            });
        }
    }
}
