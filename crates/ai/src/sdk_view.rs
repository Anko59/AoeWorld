//! The SDK's fog-filtered `World` as a `View`, mirroring
//! `crates/sdk-python/src/world.rs`.
use aoe_sdk::{BuildingKind, EntityId, ResourceKind, TileCoord, Visibility, World};

use crate::view::{Animal, Building, Economy, Node, Tile, Unit, View};

fn coord(tile: Tile) -> TileCoord {
    TileCoord::new(tile.0, tile.1)
}

fn tile(coord: TileCoord) -> Tile {
    (coord.x, coord.y)
}

impl View for World {
    fn tick(&self) -> u64 {
        World::tick(self)
    }
    fn tick_hz(&self) -> u32 {
        World::tick_hz(self)
    }
    fn map_size(&self) -> (i32, i32) {
        World::map_size(self)
    }
    fn me(&self) -> Option<u16> {
        World::me(self).map(|player| player.0)
    }
    fn economy(&self) -> Option<Economy> {
        World::economy(self).map(Economy::from)
    }
    fn diplomacy(&self) -> Option<&aoe_sdk::DiplomacyView> {
        World::diplomacy(self)
    }
    fn relics(&self) -> Option<&aoe_sdk::RelicView> {
        World::relics(self)
    }
    fn my_units(&self) -> Vec<Unit> {
        World::my_units(self, None)
            .into_iter()
            .map(Unit::from)
            .collect()
    }
    fn my_buildings(&self) -> Vec<Building> {
        World::my_buildings(self, None)
            .into_iter()
            .map(Building::from)
            .collect()
    }
    fn visible_enemies(&self) -> Vec<Unit> {
        World::visible_enemies(self)
            .into_iter()
            .map(Unit::from)
            .collect()
    }
    fn known_enemy_buildings(&self) -> Vec<Building> {
        World::known_enemy_buildings(self)
            .into_iter()
            .map(Building::from)
            .collect()
    }
    fn town_center(&self) -> Option<Building> {
        World::town_center(self).map(Building::from)
    }
    fn animals(&self) -> Vec<Animal> {
        World::animals(self).into_iter().map(Animal::from).collect()
    }
    fn resources(&self, kind: ResourceKind) -> Vec<Node> {
        World::resources(self, Some(kind))
            .into_iter()
            .map(Node::from)
            .collect()
    }
    fn visibility(&self, at: Tile) -> Visibility {
        World::visibility(self, coord(at))
    }
    fn passable(&self, at: Tile) -> Option<bool> {
        World::passable(self, coord(at))
    }
    fn land_known(&self, at: Tile) -> bool {
        World::land_known(self, coord(at))
    }
    fn land_route(&self, from: Tile, to: Tile) -> Option<Vec<Tile>> {
        World::land_route(self, coord(from), coord(to))
            .map(|path| path.into_iter().map(tile).collect())
    }
    fn land_frontier(&self, from: Tile, to: Tile) -> Option<Tile> {
        World::land_frontier(self, coord(from), coord(to)).map(tile)
    }
    fn first_reachable_monastery(&self, from: Tile, candidates: &[EntityId]) -> Option<EntityId> {
        World::first_reachable_monastery(self, coord(from), candidates)
    }
    fn find_build_site(
        &self,
        kind: BuildingKind,
        near: Tile,
        max_radius: i32,
        gap: i32,
        avoid: &[Tile],
    ) -> Option<Tile> {
        let avoid: Vec<_> = avoid.iter().map(|&tile| coord(tile)).collect();
        World::find_build_site(self, kind, coord(near), max_radius, gap, &avoid).map(tile)
    }
    fn find_bridge_site(&self, from: Tile, to: Tile) -> Option<(Tile, Tile)> {
        World::find_bridge_site(self, coord(from), coord(to)).map(|(a, b)| (tile(a), tile(b)))
    }
}
