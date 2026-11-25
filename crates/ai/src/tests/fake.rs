//! Stand-ins for the SDK world, ported from `ai/tests/fakes.py`, so decision
//! logic is testable in isolation. Entities are the crate's own `view` structs
//! built with the Python helpers' defaults.
use std::cell::RefCell;

use aoe_sdk::{
    Age, BuildingKind, CombatStance, CommandResult, Event, Order, ResourceKind, UnitKind, UnitTask,
    Visibility,
};

use crate::blackboard::Blackboard;
use crate::config::Config;
use crate::context::Context;
use crate::seat::Seat;
use crate::view::{Animal, Building, Economy, Node, Tile, Unit, View};

/// `fakes.unit`: an idle, living villager of ours with 25 hit points.
pub fn unit(id: u32, tile: Tile) -> Unit {
    Unit {
        id,
        owner: 0,
        mine: true,
        kind: UnitKind::Villager,
        x: tile.0,
        y: tile.1,
        task: UnitTask::Idle,
        hp: 25,
        max_hp: 25,
        moving: false,
        auto_scout: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        army_size: None,
        general_state: None,
        idle: true,
        alive: true,
        military: false,
        carrying: None,
        gathering: None,
        attack_target: None,
        stance: CombatStance::default(),
    }
}

/// A unit of the given kind of ours; `military` follows the Python helper.
pub fn unit_of(id: u32, kind: UnitKind, tile: Tile) -> Unit {
    let military = matches!(
        kind,
        UnitKind::Spearman | UnitKind::Archer | UnitKind::Skirmisher | UnitKind::Militia
    );
    Unit {
        kind,
        military,
        ..unit(id, tile)
    }
}

pub fn soldier(id: u32, tile: Tile) -> Unit {
    unit_of(id, UnitKind::Spearman, tile)
}

/// A foreign unit (`mine=False`, owner 1).
pub fn enemy(id: u32, kind: UnitKind, tile: Tile) -> Unit {
    Unit {
        mine: false,
        owner: 1,
        ..unit_of(id, kind, tile)
    }
}

/// `fakes.building`: a complete, visible building of ours with 1000 hit points.
pub fn building(id: u32, kind: BuildingKind, tile: Tile) -> Building {
    Building {
        id,
        owner: 0,
        mine: true,
        kind,
        x: tile.0,
        y: tile.1,
        width: kind.side(),
        height: kind.depth(),
        complete: true,
        progress: 1000,
        hp: 1000,
        max_hp: 1000,
        visible: true,
        queue_length: 0,
        queued_units: Vec::new(),
        farm_reseeds: 0,
        garrison_capacity: 0,
        garrison: Vec::new(),
        rally_position: None,
        rally_resource: None,
        rally_building: None,
        rally_animal: None,
        researching: None,
        idle: true,
    }
}

pub fn foreign_building(id: u32, kind: BuildingKind, tile: Tile) -> Building {
    Building {
        mine: false,
        owner: 1,
        ..building(id, kind, tile)
    }
}

pub fn node(id: u64, kind: ResourceKind, tile: Tile) -> Node {
    Node {
        id,
        x: tile.0,
        y: tile.1,
        kind,
        remaining: 100,
    }
}

/// `fakes.economy()` defaults: 200 food, 200 wood, 100 gold, 200 stone, 4/10 population, Dark Age.
pub fn economy() -> Economy {
    Economy {
        food: 200,
        wood: 200,
        gold: 100,
        stone: 200,
        population: 4,
        population_cap: 10,
        age: Age::Dark,
        researched: Vec::new(),
        wood_price: 100,
        food_price: 100,
        stone_price: 100,
    }
}

/// `fakes.base`: a Town Center at (50, 50), `villagers` villagers around it and a scout (id 100).
pub fn base(villagers: u32) -> (Vec<Unit>, Vec<Building>) {
    let mut units: Vec<Unit> = (1..=villagers)
        .map(|i| unit(i, (48 + (i % 4) as i32, 52 + (i / 4) as i32)))
        .collect();
    units.push(unit_of(100, UnitKind::Scout, (50, 50)));
    (
        units,
        vec![building(500, BuildingKind::TownCenter, (50, 50))],
    )
}

/// `fakes.order_result`.
pub fn order_result(sequence: u64, result: CommandResult) -> Event {
    Event::OrderResult { sequence, result }
}

/// `fakes.FakeWorld`. Fields are public so a test edits the world between decisions.
pub struct FakeWorld {
    pub tick: u64,
    pub tick_hz: u32,
    pub me: Option<u16>,
    pub size: (i32, i32),
    pub economy: Option<Economy>,
    pub units: Vec<Unit>,
    pub buildings: Vec<Building>,
    pub nodes: Vec<Node>,
    pub animals: Vec<Animal>,
    pub enemies: Vec<Unit>,
    pub enemy_buildings: Vec<Building>,
    /// When true `land_route` and `land_frontier` find no route at all.
    pub disconnected: bool,
    /// Every `land_route` the world answered, in order.
    pub route_queries: RefCell<Vec<(Tile, Tile)>>,
    /// Every `land_frontier` the world answered, in order.
    pub frontier_queries: RefCell<Vec<(Tile, Tile)>>,
    /// Every `find_build_site` the world answered (`fakes.FakeWorld.sites`).
    pub sites: RefCell<Vec<(BuildingKind, Tile)>>,
}

impl FakeWorld {
    pub fn new(units: Vec<Unit>, buildings: Vec<Building>) -> Self {
        Self {
            tick: 100,
            tick_hz: 20,
            me: Some(0),
            size: (300, 300),
            economy: Some(economy()),
            units,
            buildings,
            nodes: Vec::new(),
            animals: Vec::new(),
            enemies: Vec::new(),
            enemy_buildings: Vec::new(),
            disconnected: false,
            route_queries: RefCell::new(Vec::new()),
            frontier_queries: RefCell::new(Vec::new()),
            sites: RefCell::new(Vec::new()),
        }
    }

    /// Center tile of the first complete Town Center (the origin when there is none).
    pub fn town_center_center(&self) -> Tile {
        self.town_center().map_or((0, 0), |b| b.center())
    }

    fn known(&self, tile: Tile) -> bool {
        0 <= tile.0 && tile.0 < self.size.0 && 0 <= tile.1 && tile.1 < self.size.1
    }

    fn straight(from: Tile, to: Tile) -> Vec<Tile> {
        let (mut x, mut y) = from;
        let mut path = vec![(x, y)];
        while (x, y) != to {
            x += (to.0 > x) as i32 - (to.0 < x) as i32;
            y += (to.1 > y) as i32 - (to.1 < y) as i32;
            path.push((x, y));
        }
        path
    }
}

impl View for FakeWorld {
    fn tick(&self) -> u64 {
        self.tick
    }
    fn tick_hz(&self) -> u32 {
        self.tick_hz
    }
    fn map_size(&self) -> (i32, i32) {
        self.size
    }
    fn me(&self) -> Option<u16> {
        self.me
    }
    fn economy(&self) -> Option<Economy> {
        self.economy.clone()
    }
    fn my_units(&self) -> Vec<Unit> {
        self.units.clone()
    }
    fn my_buildings(&self) -> Vec<Building> {
        self.buildings.clone()
    }
    fn visible_enemies(&self) -> Vec<Unit> {
        self.enemies.clone()
    }
    fn known_enemy_buildings(&self) -> Vec<Building> {
        self.enemy_buildings.clone()
    }
    fn town_center(&self) -> Option<Building> {
        self.buildings
            .iter()
            .find(|b| b.kind == BuildingKind::TownCenter && b.complete)
            .cloned()
    }
    fn animals(&self) -> Vec<Animal> {
        self.animals.clone()
    }
    fn resources(&self, kind: ResourceKind) -> Vec<Node> {
        self.nodes
            .iter()
            .filter(|n| n.kind == kind)
            .cloned()
            .collect()
    }
    fn visibility(&self, tile: Tile) -> Visibility {
        if self.known(tile) {
            Visibility::Visible
        } else {
            Visibility::Unexplored
        }
    }
    fn passable(&self, tile: Tile) -> Option<bool> {
        self.known(tile).then_some(true)
    }
    fn land_known(&self, tile: Tile) -> bool {
        self.known(tile)
    }
    fn land_route(&self, from: Tile, to: Tile) -> Option<Vec<Tile>> {
        self.route_queries.borrow_mut().push((from, to));
        (!self.disconnected).then(|| Self::straight(from, to))
    }
    fn land_frontier(&self, from: Tile, to: Tile) -> Option<Tile> {
        self.frontier_queries.borrow_mut().push((from, to));
        if self.disconnected {
            None
        } else {
            Self::straight(from, to).last().copied()
        }
    }
    fn find_build_site(
        &self,
        kind: BuildingKind,
        near: Tile,
        _max_radius: i32,
        _gap: i32,
    ) -> Option<Tile> {
        self.sites.borrow_mut().push((kind, near));
        Some((near.0 + 3, near.1 + 3))
    }
    fn find_bridge_site(&self, _from: Tile, _to: Tile) -> Option<(Tile, Tile)> {
        None
    }
}

/// `fakes.FakeClient`: records orders and returns increasing sequences (1, 2, ...).
#[derive(Default)]
pub struct Recorder {
    pub orders: Vec<Order>,
}

impl Seat for Recorder {
    fn send(&mut self, order: Order) -> u64 {
        self.orders.push(order);
        self.orders.len() as u64
    }
}

/// Build a `Context` for one closure, as `Context(client, decision, board, config)` did.
pub fn with_context<R>(
    world: &FakeWorld,
    events: &[Event],
    board: &mut Blackboard,
    config: &Config,
    f: impl FnOnce(&mut Context<'_>) -> R,
) -> (R, Recorder) {
    let mut seat = Recorder::default();
    let result = {
        let mut ctx = Context::new(world, events, board, config, &mut seat);
        f(&mut ctx)
    };
    (result, seat)
}
