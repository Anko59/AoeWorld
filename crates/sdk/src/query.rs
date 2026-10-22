//! Queries over a [`World`]: what a bot asks before it decides.
use crate::{
    model::{
        Animal, Building, BuildingKind, ConstructionState, Economy, ResourceKind, ResourceNode,
        Unit, UnitKind, Visibility, distance_squared,
    },
    world::World,
};
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};

impl World {
    /// Identifies the match; it changes when the server starts a new one.
    pub fn world_id(&self) -> u64 {
        self.world_id
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn tick_hz(&self) -> u32 {
        self.tick_hz
    }
    /// Game time in whole seconds.
    pub fn seconds(&self) -> u64 {
        self.tick / u64::from(self.tick_hz.max(1))
    }
    /// Map size in tiles (width, height).
    pub fn map_size(&self) -> (i32, i32) {
        (self.width, self.height)
    }
    /// The player this slot controls, once the first snapshot told it.
    pub fn me(&self) -> Option<PlayerId> {
        self.me
    }
    /// True once the first snapshot arrived; queries are meaningful after it.
    pub fn ready(&self) -> bool {
        self.snapshot_seen
    }
    pub fn economy(&self) -> Option<Economy> {
        self.economy.map(|state| Economy { state })
    }

    fn wrap_unit(&self, state: aoe_protocol::GameplayUnitState) -> Unit {
        Unit {
            state,
            mine: Some(state.player) == self.me,
        }
    }

    pub fn unit(&self, id: EntityId) -> Option<Unit> {
        self.units.get(&id).map(|state| self.wrap_unit(*state))
    }

    /// Living units of the slot, optionally of one kind.
    pub fn my_units(&self, kind: Option<UnitKind>) -> Vec<Unit> {
        self.units
            .values()
            .map(|state| self.wrap_unit(*state))
            .filter(|unit| unit.mine && unit.alive() && kind.is_none_or(|k| unit.kind() == k))
            .collect()
    }

    /// Living own units of `kind` that are doing nothing.
    pub fn idle_units(&self, kind: Option<UnitKind>) -> Vec<Unit> {
        self.my_units(kind).into_iter().filter(Unit::idle).collect()
    }

    /// Foreign units visible right now (the server sends no others).
    pub fn visible_enemies(&self) -> Vec<Unit> {
        self.units
            .values()
            .map(|state| self.wrap_unit(*state))
            .filter(|unit| !unit.mine && unit.alive())
            .collect()
    }

    fn wrap_building(&self, state: &aoe_protocol::GameplayBuildingState) -> Building {
        let mine = Some(state.player) == self.me;
        let f = state.footprint;
        let visible = mine
            || (f.min.y..f.max.y).any(|y| {
                (f.min.x..f.max.x)
                    .any(|x| self.fog.visibility(TileCoord::new(x, y)) == Visibility::Visible)
            });
        Building {
            state: state.clone(),
            mine,
            visible,
        }
    }

    pub fn building(&self, id: EntityId) -> Option<Building> {
        self.buildings.get(&id).map(|b| self.wrap_building(b))
    }

    /// Own buildings, foundations included, optionally of one kind.
    pub fn my_buildings(&self, kind: Option<BuildingKind>) -> Vec<Building> {
        self.buildings
            .values()
            .filter(|b| Some(b.player) == self.me && kind.is_none_or(|k| b.kind == k))
            .map(|b| self.wrap_building(b))
            .collect()
    }

    /// Own buildings that are finished.
    pub fn my_complete_buildings(&self, kind: Option<BuildingKind>) -> Vec<Building> {
        self.my_buildings(kind)
            .into_iter()
            .filter(Building::complete)
            .collect()
    }

    /// Foreign buildings the slot has seen (fog memory, possibly stale).
    pub fn known_enemy_buildings(&self) -> Vec<Building> {
        self.buildings
            .values()
            .filter(|b| Some(b.player) != self.me)
            .map(|b| self.wrap_building(b))
            .collect()
    }

    /// The slot's first finished Town Center.
    pub fn town_center(&self) -> Option<Building> {
        self.my_complete_buildings(Some(BuildingKind::TownCenter))
            .into_iter()
            .next()
    }

    /// Animals the slot can see (or owns).
    pub fn animals(&self) -> Vec<Animal> {
        self.animals.values().copied().collect()
    }

    pub fn projectiles(&self) -> &[crate::model::Projectile] {
        &self.projectiles
    }

    /// Every node the slot has been told about: map chunks plus the starting
    /// kit, which only the resource feed describes.
    fn nodes(&self) -> impl Iterator<Item = crate::catalog::Node> + '_ {
        let starting = self
            .amounts
            .range(crate::catalog::STARTING_ID_BASE..)
            .filter_map(|(id, _)| crate::catalog::Node::from_starting_id(*id));
        self.catalog.nodes.values().copied().chain(starting)
    }

    /// Resource nodes on explored tiles that still hold something, with the
    /// last amount the slot saw.
    pub fn resources(&self, kind: Option<ResourceKind>) -> Vec<ResourceNode> {
        self.nodes()
            .filter(|node| kind.is_none_or(|k| node.kind == k))
            .filter(|node| self.fog.visibility(node.tile) != Visibility::Unexplored)
            .filter_map(|node| {
                let remaining = self.amounts.get(&node.id).copied().unwrap_or(node.initial);
                (remaining > 0).then_some(ResourceNode {
                    id: node.id,
                    tile: node.tile,
                    kind: node.kind,
                    remaining,
                })
            })
            .collect()
    }

    /// The explored node of `kind` closest to `from`.
    pub fn nearest_resource(&self, kind: ResourceKind, from: TileCoord) -> Option<ResourceNode> {
        self.resources(Some(kind))
            .into_iter()
            .min_by_key(|node| (distance_squared(node.tile, from), node.id))
    }

    /// How the slot sees one tile.
    pub fn visibility(&self, tile: TileCoord) -> Visibility {
        self.fog.visibility(tile)
    }

    pub fn fog(&self) -> &crate::fog::FogMemory {
        &self.fog
    }

    /// Whether ground units can stand on `tile`; `None` until its map chunk loaded.
    pub fn passable(&self, tile: TileCoord) -> Option<bool> {
        self.catalog.passable(tile)
    }

    /// Chunks of map data the SDK has read so far.
    pub fn loaded_chunks(&self) -> usize {
        self.catalog.loaded_chunks()
    }

    /// A tile for a new `kind` building near `near`, or `None`: every footprint
    /// tile explored, walkable and free of resources and known buildings, with
    /// `gap` free tiles around so villagers can still pass.
    pub fn find_build_site(
        &self,
        kind: BuildingKind,
        near: TileCoord,
        max_radius: i32,
        gap: i32,
    ) -> Option<TileCoord> {
        let side = kind.side();
        let depth = kind.depth();
        let blocked: Vec<TileRect> = self
            .buildings
            .values()
            .map(|b| b.footprint)
            .chain(self.nodes().filter_map(|node| {
                let remaining = self.amounts.get(&node.id).copied().unwrap_or(node.initial);
                (remaining > 0).then(|| TileRect::from_xywh(node.tile.x, node.tile.y, 1, 1))
            }))
            .collect();
        for radius in 0..=max_radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let origin = TileCoord::new(near.x + dx, near.y + dy);
                    let rect = TileRect::from_xywh(origin.x, origin.y, side, depth);
                    if self.site_is_free(rect, gap, &blocked) {
                        return Some(origin);
                    }
                }
            }
        }
        None
    }

    fn site_is_free(&self, rect: TileRect, gap: i32, blocked: &[TileRect]) -> bool {
        if rect.min.x < 2
            || rect.min.y < 2
            || rect.max.x > self.width - 2
            || rect.max.y > self.height - 2
        {
            return false;
        }
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                let tile = TileCoord::new(x, y);
                if self.fog.visibility(tile) == Visibility::Unexplored
                    || self.catalog.passable(tile) != Some(true)
                {
                    return false;
                }
            }
        }
        let grown = TileRect::new(
            TileCoord::new(rect.min.x - gap, rect.min.y - gap),
            TileCoord::new(rect.max.x + gap, rect.max.y + gap),
        );
        !blocked.iter().any(|other| {
            other.min.x < grown.max.x
                && other.max.x > grown.min.x
                && other.min.y < grown.max.y
                && other.max.y > grown.min.y
        })
    }

    /// Own foundations that are not finished and have no one building them.
    pub fn unfinished_buildings(&self) -> Vec<Building> {
        self.my_buildings(None)
            .into_iter()
            .filter(|b| b.state.construction == ConstructionState::UnderConstruction)
            .collect()
    }
}
