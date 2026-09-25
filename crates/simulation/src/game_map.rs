use crate::{
    GameWorld, GameWorldError, MAX_ROUTE_WORK_PER_TICK, Terrain, movement_speed::cavalry_config,
    navigation_cache::NavigationCache,
};
use aoe_core::{Seed, Tick, WorldConfig};
use aoe_map::EnvironmentPageProvider;
use aoe_map::{ElevationPage, HistoricalLandUsePage, MapPackage, PotentialBiomePage, WaterPage};
use std::collections::BTreeMap;
use std::sync::Arc;

impl GameWorld {
    pub fn from_map(package: MapPackage) -> Result<Self, GameWorldError> {
        let width = i32::try_from(package.estimate.tiles_per_side)
            .map_err(|_| GameWorldError::InvalidPosition)?;
        let config = cavalry_config(WorldConfig::new(width, width, Seed(package.request.seed))?);
        Ok(Self {
            terrain: Terrain::from_package(&package),
            config,
            tick: Tick(0),
            units: Vec::new(),
            lookup: BTreeMap::new(),
            chunks: BTreeMap::new(),
            active_movers: Vec::new(),
            planning_budget: MAX_ROUTE_WORK_PER_TICK,
            navigation_cache: NavigationCache::default(),
            planning_cursor: 0,
            active_planner_count: 0,
            active_route_searches: 0,
        })
    }

    pub fn from_prepared_map(
        package: MapPackage,
        elevation_pages: Vec<ElevationPage>,
        water_pages: Vec<WaterPage>,
        vegetation_pages: Vec<PotentialBiomePage>,
        land_use_pages: Vec<HistoricalLandUsePage>,
    ) -> Result<Self, GameWorldError> {
        let width = i32::try_from(package.estimate.tiles_per_side)
            .map_err(|_| GameWorldError::InvalidPosition)?;
        let config = cavalry_config(WorldConfig::new(width, width, Seed(package.request.seed))?);
        Ok(Self {
            terrain: Terrain::from_prepared_package(
                &package,
                elevation_pages,
                water_pages,
                vegetation_pages,
                land_use_pages,
            )
            .map_err(|_| GameWorldError::InvalidTerrain)?,
            config,
            tick: Tick(0),
            units: Vec::new(),
            lookup: BTreeMap::new(),
            chunks: BTreeMap::new(),
            active_movers: Vec::new(),
            planning_budget: MAX_ROUTE_WORK_PER_TICK,
            navigation_cache: NavigationCache::default(),
            planning_cursor: 0,
            active_planner_count: 0,
            active_route_searches: 0,
        })
    }

    pub fn from_page_provider(
        package: MapPackage,
        provider: Arc<dyn EnvironmentPageProvider>,
    ) -> Result<Self, GameWorldError> {
        let width = i32::try_from(package.estimate.tiles_per_side)
            .map_err(|_| GameWorldError::InvalidPosition)?;
        let config = cavalry_config(WorldConfig::new(width, width, Seed(package.request.seed))?);
        Ok(Self {
            terrain: Terrain::from_page_provider(&package, provider)
                .map_err(|_| GameWorldError::InvalidTerrain)?,
            config,
            tick: Tick(0),
            units: Vec::new(),
            lookup: BTreeMap::new(),
            chunks: BTreeMap::new(),
            active_movers: Vec::new(),
            planning_budget: MAX_ROUTE_WORK_PER_TICK,
            navigation_cache: NavigationCache::default(),
            planning_cursor: 0,
            active_planner_count: 0,
            active_route_searches: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::{EnvironmentPage, EnvironmentPageError, EnvironmentPageKey};

    #[derive(Debug)]
    struct UnusedProvider;

    impl EnvironmentPageProvider for UnusedProvider {
        fn page(
            &self,
            _key: EnvironmentPageKey,
            _cancelled: &dyn Fn() -> bool,
        ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
            panic!("provider must not be queried while constructing a world");
        }
    }

    #[test]
    fn prepared_map_constructor_builds_the_same_bounded_world_as_a_package() {
        let package =
            MapPackage::new(1, aoe_map::MapRequest::default(), Vec::new()).expect("package");
        let world =
            GameWorld::from_prepared_map(package, Vec::new(), Vec::new(), Vec::new(), Vec::new())
                .expect("prepared world");
        assert!(world.terrain().has_map_navigation());
        assert_eq!(world.config().width_tiles, world.config().height_tiles);
    }

    #[test]
    fn page_provider_requires_an_environment_index_before_binding() {
        let package =
            MapPackage::new(1, aoe_map::MapRequest::default(), Vec::new()).expect("package");
        assert!(matches!(
            GameWorld::from_page_provider(package, Arc::new(UnusedProvider)),
            Err(GameWorldError::InvalidTerrain)
        ));
    }
}
