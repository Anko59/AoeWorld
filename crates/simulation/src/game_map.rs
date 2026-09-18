use crate::{GameWorld, GameWorldError, Terrain};
use aoe_core::{Seed, Tick, WorldConfig};
use aoe_map::MapPackage;
use std::collections::BTreeMap;

impl GameWorld {
    pub fn from_map(package: MapPackage) -> Result<Self, GameWorldError> {
        let width = i32::try_from(package.estimate.tiles_per_side)
            .map_err(|_| GameWorldError::InvalidPosition)?;
        let config = WorldConfig::new(width, width, Seed(package.request.seed))?;
        Ok(Self {
            terrain: Terrain::from_package(&package),
            config,
            tick: Tick(0),
            units: Vec::new(),
            lookup: BTreeMap::new(),
            chunks: BTreeMap::new(),
            active_movers: Vec::new(),
        })
    }
}
