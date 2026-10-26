//! Trial DAT measurements plus current DE overrides (see docs/fishing.md).
use crate::{BuildingKind, EconomyError, GameWorld, GameWorldError, Terrain, UnitKind};
use aoe_core::{TileCoord, TileRect};
use aoe_map::{EnvironmentPageError, GroundMaterial, ObjectKind, WaterKind};
pub(super) mod navigation;

impl GameWorld {
    pub(crate) fn nearest_fish(
        &self,
        worker: aoe_core::EntityId,
        origin: TileCoord,
        exclude: u64,
    ) -> Option<aoe_map::ResourceNode> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        let naval = self.unit(worker)?.kind == UnitKind::FishingShip;
        let mut nearest = None;
        let mut score = (u64::MAX, u64::MAX);
        let radius = super::VILLAGE_RULES.replacement_radius_tiles;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let tile = TileCoord::new(origin.x + dx, origin.y + dy);
                let distance = super::placement::distance_squared(origin, tile);
                if distance > (radius as u64).pow(2) {
                    continue;
                }
                if let Some(node) = generator.object_at(tile)
                    && node.id != exclude
                    && overlay.resource_available(node)
                    && (node.object == ObjectKind::ShoreFish
                        || (naval && node.object == ObjectKind::DeepFish))
                    && (distance, node.id) < score
                {
                    score = (distance, node.id);
                    nearest = Some(node);
                }
            }
        }
        nearest
    }
    pub fn validate_building_placement(
        &self,
        kind: BuildingKind,
        origin: TileCoord,
    ) -> Result<(), EconomyError> {
        let side = kind.rules().side;
        let footprint = TileRect::new(
            origin,
            TileCoord::new(
                origin.x.saturating_add(side),
                origin.y.saturating_add(kind.rules().depth),
            ),
        );
        if kind == BuildingKind::Bridge {
            return Err(EconomyError::InvalidFootprint);
        }
        if kind != BuildingKind::Dock {
            return self.validate_footprint(footprint);
        }
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(EconomyError::InvalidFootprint);
        };
        if footprint.min.x < 0
            || footprint.min.y < 0
            || footprint.max.x > self.config.width_tiles
            || footprint.max.y > self.config.height_tiles
            || self
                .units()
                .any(|u| !self.embarked(u.id) && footprint.contains(u.position.tile_floor()))
            || self
                .buildings()
                .any(|b| super::placement::rects_overlap(b.footprint, footprint))
        {
            return Err(EconomyError::InvalidFootprint);
        }
        let (mut land, mut water) = (false, false);
        for y in footprint.min.y..footprint.max.y {
            for x in footprint.min.x..footprint.max.x {
                let tile = TileCoord::new(x, y);
                let sample = generator
                    .tile_at_with_cancel(tile, &|| false)
                    .map_err(GameWorldError::Environment)?
                    .ok_or(EconomyError::InvalidFootprint)?;
                let wet = navigable(sample.water, sample.material);
                if overlay.blocks_tile(tile)
                    || (!wet && !sample.passable)
                    || generator
                        .object_at_with_cancel(tile, &|| false)
                        .map_err(GameWorldError::Environment)?
                        .is_some_and(|n| overlay.blocks_node(n))
                {
                    return Err(EconomyError::InvalidFootprint);
                }
                water |= wet;
                land |= !wet && sample.water == WaterKind::None;
            }
        }
        if land && water {
            Ok(())
        } else {
            Err(EconomyError::InvalidFootprint)
        }
    }

    pub(crate) fn unit_tile_passable(
        &self,
        kind: UnitKind,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if !kind.is_naval() {
            return self
                .terrain
                .passable_with_cancel(tile, self.config, cancelled);
        }
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Ok(false);
        };
        let Some((sample, node)) = generator.tile_and_node_with_cancel(tile, cancelled)? else {
            return Ok(false);
        };
        Ok(navigable(sample.water, sample.material)
            && !overlay.blocks_tile(tile)
            && overlay.bridge_axis(tile).is_none()
            && node.is_none_or(|n| !overlay.blocks_node(n)))
    }

    pub(crate) fn gather_capacity(&self, unit: &crate::GameUnit, farmer: bool) -> u16 {
        if unit.kind == UnitKind::FishingShip {
            15
        } else {
            self.villager_capacity(unit.player)
                + u16::from(farmer && self.has_researched(unit.player, crate::Tech::HeavyPlow))
        }
    }

    pub(crate) fn fish_object(&self, id: u64) -> Option<ObjectKind> {
        let Terrain::Map { generator, .. } = &self.terrain else {
            return None;
        };
        generator
            .resource_by_id(id)
            .map(|n| n.object)
            .filter(|o| matches!(o, ObjectKind::ShoreFish | ObjectKind::DeepFish))
    }
}

pub(super) fn navigable(water: WaterKind, material: GroundMaterial) -> bool {
    matches!(water, WaterKind::Ocean | WaterKind::Lake | WaterKind::River)
        && material != GroundMaterial::Ford
}
