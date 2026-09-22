use super::{
    Biome, GroundMaterial, MapChunkGenerator, ObjectKind, Provenance, ResourceKind, ResourceNode,
    Tile, WaterKind, compression_fallback, quantize_game_height, resource_id, resources,
    signed_noise, surface, unsigned_noise,
};
use crate::biome_rules::{biome_from_potential_class, material_for, tree_present};
use aoe_core::TileCoord;

impl MapChunkGenerator {
    pub(super) fn sample_tile(&self, tile: TileCoord) -> Tile {
        let broad = signed_noise(
            self.geography_key,
            b"relief",
            tile.x.div_euclid(8),
            tile.y.div_euclid(8),
        );
        let local = signed_noise(self.geography_key, b"relief-detail", tile.x, tile.y) / 8;
        let fallback_height = broad.saturating_mul(25).saturating_add(local);
        let fallback_water = if unsigned_noise(
            self.geography_key,
            b"water",
            tile.x.div_euclid(16),
            tile.y.div_euclid(16),
        )
        .is_multiple_of(97)
        {
            WaterKind::Lake
        } else if unsigned_noise(
            self.geography_key,
            b"river",
            tile.x.div_euclid(4),
            tile.y.div_euclid(4),
        )
        .is_multiple_of(521)
        {
            WaterKind::River
        } else {
            WaterKind::None
        };
        let fallback_biome = match unsigned_noise(
            self.geography_key,
            b"biome",
            tile.x.div_euclid(32),
            tile.y.div_euclid(32),
        ) % 10
        {
            0 => Biome::Tropical,
            1 => Biome::Boreal,
            2 => Biome::Woodland,
            3 => Biome::Savanna,
            4 => Biome::Steppe,
            5 => Biome::Desert,
            6 => Biome::Tundra,
            7 => Biome::Alpine,
            8 => Biome::Polar,
            _ => Biome::Temperate,
        };
        let (biome, vegetation_provenance) = self
            .biome
            .as_ref()
            .and_then(|biome| biome.class_at(tile, self.width_tiles))
            .and_then(biome_from_potential_class)
            .map(|biome| (biome, Provenance::SourceDerived))
            .unwrap_or((fallback_biome, Provenance::Fallback));
        let (
            geographic_height_centimeters,
            game_height_level,
            surface,
            elevation_provenance,
            water,
            water_provenance,
        ) = self
            .elevation
            .as_ref()
            .and_then(|elevation| {
                elevation
                    .height_at(tile, self.width_tiles)
                    .map(|height| (height, elevation.compression))
            })
            .map(|(height, compression)| {
                let game_height = quantize_game_height(height, compression);
                let corner_heights = self
                    .elevation
                    .as_ref()
                    .and_then(|elevation| elevation.corner_heights(tile, self.width_tiles))
                    .unwrap_or([height; 4]);
                (
                    height,
                    game_height,
                    surface::from_heights(corner_heights, compression),
                    Provenance::SourceDerived,
                    // Elevation cannot identify water: inland depressions can be dry,
                    // while coastlines require independent, coherent water geometry.
                    fallback_water,
                    Provenance::Fallback,
                )
            })
            .unwrap_or((
                fallback_height,
                quantize_game_height(fallback_height, compression_fallback()),
                surface::from_heights([fallback_height; 4], compression_fallback()),
                Provenance::Fallback,
                fallback_water,
                Provenance::Fallback,
            ));
        let (water, water_provenance) = self
            .water
            .as_ref()
            .and_then(|water| water.coverage_at(tile, self.width_tiles))
            .map(|coverage| match coverage.ocean_percent {
                1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
                51..=100 => (WaterKind::Ocean, Provenance::SourceDerived),
                _ => match coverage.inland_percent {
                    0 => (WaterKind::None, Provenance::SourceDerived),
                    1..=50 => (WaterKind::Shallow, Provenance::SourceDerived),
                    _ => (WaterKind::Lake, Provenance::SourceDerived),
                },
            })
            .unwrap_or((water, water_provenance));
        let material = match water {
            WaterKind::None => material_for(biome, geographic_height_centimeters),
            WaterKind::River | WaterKind::Lake | WaterKind::Ocean => GroundMaterial::Water,
            WaterKind::Shallow => GroundMaterial::Shore,
        };
        Tile {
            geographic_height_centimeters,
            game_height_level,
            surface,
            material,
            biome,
            vegetation_provenance,
            water,
            elevation_provenance,
            water_provenance,
            passable: water == WaterKind::None
                && material != GroundMaterial::Ice
                && surface.walkable(),
        }
    }

    pub(super) fn resource_at(&self, tile: TileCoord, sample: Tile) -> Option<ResourceNode> {
        if !sample.passable {
            return None;
        }
        if let Some(tree) = self.tree_at(tile, sample) {
            return Some(tree);
        }
        resources::at(self, tile, sample)
    }

    pub(super) fn occupied_without_access(&self, tile: TileCoord, sample: Tile) -> bool {
        !sample.passable
            || self.tree_at(tile, sample).is_some()
            || resources::candidate(self, tile, sample).is_some()
    }

    fn tree_at(&self, tile: TileCoord, sample: Tile) -> Option<ResourceNode> {
        let value = unsigned_noise(self.geography_key, b"objects", tile.x, tile.y)
            ^ self.procedural_seed.rotate_left(17);
        let historically_cleared = self
            .historical_land_use
            .as_ref()
            .and_then(|land_use| land_use.at(tile, self.width_tiles))
            .is_some_and(|land_use| {
                self.is_tree_suppressed_by_historical_land_use(
                    tile,
                    land_use.crop_percent,
                    land_use.grazing_percent,
                )
            });
        (!historically_cleared && tree_present(self.geography_key, tile.x, tile.y, sample.biome))
            .then_some(ResourceNode {
                id: resource_id(tile, 0),
                tile,
                kind: ResourceKind::Wood,
                object: ObjectKind::Tree,
                initial_amount: 100,
                visual_variant: (value >> 8) as u8,
            })
    }
}
