use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    EdgePassability, ElevationPage, HistoricalLandUsePage, MapChunkGenerator, MapPackage,
    MovementOutcome, PotentialBiomePage, ResourceOverlay, WaterPage, find_path_with_overlay,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UniformGrass {
    seed: u64,
}

impl UniformGrass {
    pub const fn new(seed: u64) -> Self {
        Self { seed }
    }

    pub fn material_at(self, tile: TileCoord, config: WorldConfig) -> Option<u8> {
        if tile.x < 0 || tile.y < 0 || tile.x >= config.width_tiles || tile.y >= config.height_tiles
        {
            return None;
        }
        let x = tile.x.rem_euclid(8) as u64;
        let y = tile.y.rem_euclid(8) as u64;
        Some(((x.wrapping_mul(37) + y.wrapping_mul(17) + self.seed) % 8) as u8)
    }
}

#[derive(Debug)]
pub enum Terrain {
    Uniform(UniformGrass),
    Map {
        generator: MapChunkGenerator,
        overlay: ResourceOverlay,
    },
}

impl Terrain {
    pub const fn uniform(seed: u64) -> Self {
        Self::Uniform(UniformGrass::new(seed))
    }

    pub fn from_package(package: &MapPackage) -> Self {
        Self::Map {
            generator: package.generator(),
            overlay: ResourceOverlay::default(),
        }
    }

    pub fn from_prepared_package(
        package: &MapPackage,
        elevation_pages: Vec<ElevationPage>,
        water_pages: Vec<WaterPage>,
        vegetation_pages: Vec<PotentialBiomePage>,
        land_use_pages: Vec<HistoricalLandUsePage>,
    ) -> Result<Self, aoe_map::MapPackageError> {
        Ok(Self::Map {
            generator: package.generator_with_environment(
                elevation_pages,
                water_pages,
                vegetation_pages,
                land_use_pages,
            )?,
            overlay: ResourceOverlay::default(),
        })
    }

    pub fn passable(&self, tile: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => {
                tile.x >= 0
                    && tile.y >= 0
                    && tile.x < config.width_tiles
                    && tile.y < config.height_tiles
            }
            Self::Map { generator, overlay } => generator.tile_at(tile).is_some_and(|sample| {
                sample.passable
                    && generator
                        .object_at(tile)
                        .is_none_or(|node| !overlay.blocks(generator, node.id))
            }),
        }
    }

    pub fn crossable(&self, from: TileCoord, to: TileCoord, config: WorldConfig) -> bool {
        match self {
            Self::Uniform(_) => self.passable(from, config) && self.passable(to, config),
            Self::Map { generator, overlay } => map_crossable(generator, overlay, from, to),
        }
    }

    pub fn route(&self, origin: TileCoord, destination: TileCoord) -> Option<Vec<TileCoord>> {
        let Self::Map { generator, overlay } = self else {
            return None;
        };
        match find_path_with_overlay(generator, overlay, origin, destination, 4_096) {
            MovementOutcome::Path(path) => Some(path.tiles),
            MovementOutcome::InvalidDestination
            | MovementOutcome::Unreachable
            | MovementOutcome::BudgetExceeded => Some(Vec::new()),
        }
    }
}

fn map_crossable(
    generator: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    from: TileCoord,
    to: TileCoord,
) -> bool {
    let step_clear = |from, to| {
        matches!(generator.edge_between(from, to), EdgePassability::Passable)
            && generator
                .object_at(to)
                .is_none_or(|node| !overlay.blocks(generator, node.id))
    };
    if !step_clear(from, to) {
        return false;
    }
    let diagonal = from.x != to.x && from.y != to.y;
    !diagonal
        || (step_clear(from, TileCoord::new(to.x, from.y))
            && step_clear(from, TileCoord::new(from.x, to.y)))
}
