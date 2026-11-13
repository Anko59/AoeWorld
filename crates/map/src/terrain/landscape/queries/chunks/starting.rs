//! Supplemental trees use exactly the forest's source/stand selection.
use super::*;

impl MapChunkGenerator {
    pub fn starting_tree_at(
        &self,
        position: TileCoord,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        let Some(base) = self.base_physical_tile_with_cancel(position, &|| false)? else {
            return Ok(None);
        };
        let family = families::source_tree_family(
            self.geography_key,
            self.procedural_seed,
            self.source_biome_class_with_cancel(position, &|| false)?,
            base.biome,
            position,
        );
        Ok(ResourceNode::starting_tree(position, family))
    }
}
