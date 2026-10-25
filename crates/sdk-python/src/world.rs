//! `World`: everything the slot knows, with the SDK's queries.
use crate::{
    enums::{BuildingKind, ResourceKind, UnitKind, Visibility},
    types::{Animal, Building, Economy, ResourceNode, Unit},
};
use aoe_sdk::{EntityId, TileCoord};
use pyo3::prelude::*;

/// An immutable snapshot taken at a decision point. Queries never block and
/// never see more than fog of war allows.
#[pyclass(frozen, module = "aoeworld")]
#[derive(Clone)]
pub struct World {
    pub inner: aoe_sdk::World,
}

fn units(list: Vec<aoe_sdk::Unit>) -> Vec<Unit> {
    list.into_iter().map(Unit::from).collect()
}

fn buildings(list: Vec<aoe_sdk::Building>) -> Vec<Building> {
    list.into_iter().map(Building::from).collect()
}

#[pymethods]
impl World {
    /// The game clock, in 20 Hz ticks.
    #[getter]
    fn tick(&self) -> u64 {
        self.inner.tick()
    }
    #[getter]
    fn tick_hz(&self) -> u32 {
        self.inner.tick_hz()
    }
    /// Game time in whole seconds.
    #[getter]
    fn seconds(&self) -> u64 {
        self.inner.seconds()
    }
    /// Map size in tiles, `(width, height)`.
    #[getter]
    fn map_size(&self) -> (i32, i32) {
        self.inner.map_size()
    }
    /// This slot's player id.
    #[getter]
    fn me(&self) -> Option<u16> {
        self.inner.me().map(|player| player.0)
    }
    #[getter]
    fn economy(&self) -> Option<Economy> {
        self.inner.economy().map(Economy::from)
    }
    #[pyo3(signature = (kind=None))]
    fn my_units(&self, kind: Option<UnitKind>) -> Vec<Unit> {
        units(self.inner.my_units(kind.map(Into::into)))
    }
    /// Own units of `kind` that are doing nothing.
    #[pyo3(signature = (kind=None))]
    fn idle_units(&self, kind: Option<UnitKind>) -> Vec<Unit> {
        units(self.inner.idle_units(kind.map(Into::into)))
    }
    /// Foreign units visible right now.
    fn visible_enemies(&self) -> Vec<Unit> {
        units(self.inner.visible_enemies())
    }
    fn unit(&self, id: u32) -> Option<Unit> {
        self.inner.unit(EntityId(id)).map(Unit::from)
    }
    /// Queued destinations in fixed-point subunits, including the active leg.
    fn move_queue(&self, unit:u32)->Vec<(i32,i32)> {
        self.inner.move_queue(EntityId(unit)).iter().map(|p|(p.x,p.y)).collect()
    }
    /// Own buildings, foundations included unless `complete_only`.
    #[pyo3(signature = (kind=None, complete_only=false))]
    fn my_buildings(&self, kind: Option<BuildingKind>, complete_only: bool) -> Vec<Building> {
        let kind = kind.map(Into::into);
        buildings(if complete_only {
            self.inner.my_complete_buildings(kind)
        } else {
            self.inner.my_buildings(kind)
        })
    }
    /// Foreign buildings seen so far (remembered; `visible` says if in sight).
    fn known_enemy_buildings(&self) -> Vec<Building> {
        buildings(self.inner.known_enemy_buildings())
    }
    fn building(&self, id: u32) -> Option<Building> {
        self.inner.building(EntityId(id)).map(Building::from)
    }
    fn town_center(&self) -> Option<Building> {
        self.inner.town_center().map(Building::from)
    }
    /// Own foundations that are not finished yet.
    fn unfinished_buildings(&self) -> Vec<Building> {
        buildings(self.inner.unfinished_buildings())
    }
    /// Wild and owned animals in sight.
    fn animals(&self) -> Vec<Animal> {
        self.inner.animals().into_iter().map(Animal::from).collect()
    }
    /// Explored resource nodes that still hold something.
    #[pyo3(signature = (kind=None))]
    fn resources(&self, kind: Option<ResourceKind>) -> Vec<ResourceNode> {
        self.inner
            .resources(kind.map(Into::into))
            .into_iter()
            .map(ResourceNode::from)
            .collect()
    }
    /// The explored node of `kind` nearest to tile `(x, y)`.
    fn nearest_resource(&self, kind: ResourceKind, x: i32, y: i32) -> Option<ResourceNode> {
        self.inner
            .nearest_resource(kind.into(), TileCoord::new(x, y))
            .map(ResourceNode::from)
    }
    fn visibility(&self, x: i32, y: i32) -> Visibility {
        self.inner.visibility(TileCoord::new(x, y)).into()
    }
    /// Map chunks (32x32 tiles) the SDK has read so far, for diagnostics.
    #[getter]
    fn loaded_chunks(&self) -> usize {
        self.inner.loaded_chunks()
    }
    /// Whether ground units can stand on the tile; `None` until its map chunk loaded.
    fn passable(&self, x: i32, y: i32) -> Option<bool> {
        self.inner.passable(TileCoord::new(x, y))
    }
    /// A bounded route over explored public land; unknown or disconnected returns None.
    fn land_route(&self, x: i32, y: i32, tx: i32, ty: i32) -> Option<Vec<(i32, i32)>> {
        self.inner.land_route(TileCoord::new(x, y), TileCoord::new(tx, ty))
            .map(|path| path.into_iter().map(|tile| (tile.x, tile.y)).collect())
    }
    /// An explored reachable frontier toward a goal, without crossing unknown water.
    fn land_frontier(&self, x: i32, y: i32, tx: i32, ty: i32) -> Option<(i32, i32)> {
        self.inner.land_frontier(TileCoord::new(x, y), TileCoord::new(tx, ty))
            .map(|tile| (tile.x, tile.y))
    }
    /// A top-left tile for a new building near `(x, y)`: explored, walkable,
    /// clear of resources and buildings, with `gap` free tiles around it.
    #[pyo3(signature = (kind, x, y, max_radius=24, gap=1))]
    fn find_build_site(
        &self,
        kind: BuildingKind,
        x: i32,
        y: i32,
        max_radius: i32,
        gap: i32,
    ) -> Option<(i32, i32)> {
        self.inner
            .find_build_site(kind.into(), TileCoord::new(x, y), max_radius, gap)
            .map(|tile| (tile.x, tile.y))
    }
    fn __repr__(&self) -> String {
        format!(
            "World(tick={}, me={:?}, units={}, buildings={})",
            self.inner.tick(),
            self.inner.me().map(|p| p.0),
            self.inner.my_units(None).len(),
            self.inner.my_buildings(None).len()
        )
    }
}
