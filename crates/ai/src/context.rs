//! One decision's view: the world, the budget and the order helpers
//! (`ai/aoe_ai/context.py`).
mod land;
mod manager;

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use aoe_sdk::{Age, BuildingKind, EntityId, Event, Order, ResourceKind, UnitKind};

use crate::blackboard::{Blackboard, Settlements, Tracked, slot};
use crate::config::Config;
use crate::geometry::{clamp_tile, dist};
use crate::roster::{land_soldier, profile, warship};
use crate::seat::Seat;
use crate::view::{Building, Economy, Node, Tile, Unit, View};

/// Tile distances one memo keeps before starting over.
const DROPOFF_MEMO: usize = 65_536;

/// Ends a manager's admitted work before it sends an excess order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderBudgetExhausted;

/// A cost as food, wood, gold, stone.
pub type Cost = [u32; 4];

/// Soldiers per unit line, in the order the lines were first counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lines(Vec<(UnitKind, u32)>);

impl Lines {
    pub fn add(&mut self, line: UnitKind, count: u32) {
        match self.0.iter_mut().find(|(kind, _)| *kind == line) {
            Some((_, n)) => *n += count,
            None => self.0.push((line, count)),
        }
    }

    pub fn get(&self, line: UnitKind) -> u32 {
        self.0
            .iter()
            .find(|(kind, _)| *kind == line)
            .map_or(0, |(_, n)| *n)
    }

    pub fn values(&self) -> impl Iterator<Item = &u32> {
        self.0.iter().map(|(_, n)| n)
    }

    pub fn iter(&self) -> impl Iterator<Item = (UnitKind, u32)> {
        self.0.iter().copied()
    }
}

/// The resources that take each kind as a drop-off.
fn dropoff_kinds(kind: ResourceKind) -> [BuildingKind; 2] {
    let camp = match kind {
        ResourceKind::Food => BuildingKind::Mill,
        ResourceKind::Wood => BuildingKind::LumberCamp,
        ResourceKind::Gold | ResourceKind::Stone => BuildingKind::MiningCamp,
    };
    [BuildingKind::TownCenter, camp]
}

/// Units an order gives work to (Python's `UNIT_ORDERS`).
fn worked_units(order: &Order) -> Vec<u32> {
    let ids = |list: &[EntityId]| list.iter().map(|id| id.0).collect();
    match order {
        Order::Gather { unit, .. }
        | Order::Farm { unit, .. }
        | Order::Move { unit, .. }
        | Order::Attack { unit, .. }
        | Order::Stop { unit }
        | Order::Garrison { unit, .. }
        | Order::Relic(aoe_sdk::RelicAction::PickUp { monk: unit, .. })
        | Order::Relic(aoe_sdk::RelicAction::Drop { monk: unit }) => vec![unit.0],
        Order::Build { builders, .. }
        | Order::BuildBridge { builders, .. }
        | Order::Construct { builders, .. } => ids(builders),
        Order::Hunt { units, .. } | Order::FormationMove { units, .. } => ids(units),
        _ => Vec::new(),
    }
}

/// Everything a manager needs for one decision, computed once.
pub struct Context<'a> {
    pub world: &'a dyn View,
    pub events: &'a [Event],
    pub board: &'a mut Blackboard,
    pub config: &'a Config,
    seat: &'a mut dyn Seat,
    pub tick: u64,
    pub economy: Option<Economy>,
    pub units: Vec<Unit>,
    pub villagers: Vec<Unit>,
    pub late_economy: bool,
    pub scouts: Vec<Unit>,
    /// Soldiers a General leads take his orders, not ours.
    pub led: Vec<Unit>,
    pub army: Vec<Unit>,
    pub buildings: Vec<Building>,
    pub enemies: Vec<Unit>,
    pub center: Option<Building>,
    pub home: Option<Tile>,
    pub queued: u32,
    pub spent: Cost,
    /// Set by a manager that needs the Town Center's queue to drain.
    pub hold_town_center: bool,
    pub saving_castle: bool,
    pub hold_military_training: bool,
    /// Producer IDs held for research this decision.
    pub draining: BTreeSet<u32>,
    pub production_drain: bool,
    /// Units that already got an order this decision.
    pub busy: BTreeSet<u32>,
    resources: [Option<Vec<Node>>; 4],
    spots: [Option<Vec<Tile>>; 4],
    land_routes: BTreeMap<(Tile, Tile), Option<Vec<Tile>>>,
    land_queries: u32,
    pub land_query_limit: u32,
    site_queries: u32,
    pub orders: u32,
    pub order_limit: u32,
    pub centers: Vec<Tile>,
    pub reserved_workers: BTreeSet<u32>,
    lines: OnceCell<Lines>,
}

impl<'a> Context<'a> {
    pub fn new(
        world: &'a dyn View,
        events: &'a [Event],
        board: &'a mut Blackboard,
        config: &'a Config,
        seat: &'a mut dyn Seat,
    ) -> Self {
        let tick = world.tick();
        let economy = world.economy();
        let units = world.my_units();
        let villagers: Vec<Unit> = units
            .iter()
            .filter(|u| u.kind == UnitKind::Villager)
            .cloned()
            .collect();
        let castle = economy
            .as_ref()
            .is_some_and(|e| matches!(e.age, Age::Castle | Age::Imperial));
        let late_economy = castle && villagers.len() >= config.late_economy_min_villagers;
        let scouts = units
            .iter()
            .filter(|u| u.kind == UnitKind::Scout)
            .cloned()
            .collect();
        let military: Vec<&Unit> = units.iter().filter(|u| land_soldier(u.kind)).collect();
        let led = military
            .iter()
            .filter(|u| u.commanded_by.is_some())
            .map(|u| (*u).clone())
            .collect();
        let army: Vec<Unit> = military
            .iter()
            .filter(|u| u.commanded_by.is_none())
            .map(|u| (*u).clone())
            .collect();
        let buildings = world.my_buildings();
        let enemies = world.visible_enemies();
        let center = world.town_center();
        if let Some(c) = &center {
            board.home = Some(c.center());
        } else if let Some(v) = villagers.first() {
            board.home = Some(v.tile());
        }
        let home = center.as_ref().map(Building::center).or(board.home);
        let queued = buildings.iter().map(|b| b.queue_length as u32).sum();
        let centers: Vec<Tile> = buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::TownCenter && b.complete)
            .map(Building::center)
            .collect();
        // Every candidate node is measured against every Town Center: the
        // distances are kept while the sites stay the same.
        let sites = (centers.clone(), home);
        if board.settlements.sites.as_ref() != Some(&sites)
            || board.settlements.distances.len() > DROPOFF_MEMO
        {
            board.settlements = Settlements {
                sites: Some(sites),
                distances: BTreeMap::new(),
            };
        }
        let managers: BTreeSet<u32> = units
            .iter()
            .map(|u| u.id)
            .chain(
                buildings
                    .iter()
                    .flat_map(|b| b.garrison.iter().map(|g| g.0)),
            )
            .collect();
        board.generals.retain(|i| managers.contains(i));
        let building_ids: BTreeSet<u32> = buildings.iter().map(|b| b.id).collect();
        board
            .seated
            .retain(|i, target| managers.contains(i) && building_ids.contains(target));
        board.manager_settings.retain(|i, _| managers.contains(i));
        board.general_training.retain(|i, _| managers.contains(i));
        // Recovery: until a Town Center is complete, Vassal reservations are
        // released so the survivors can rebuild one.
        let recovering = centers.is_empty();
        let reserved_workers = if recovering {
            BTreeSet::new()
        } else {
            board.vassal_workers.values().flatten().copied().collect()
        };
        let army_ids: BTreeSet<u32> = army.iter().filter(|u| u.alive).map(|u| u.id).collect();
        board.marched.retain(|i, _| army_ids.contains(i));
        board.land_failures.retain(|_, until| *until > tick);
        Self {
            world,
            events,
            board,
            config,
            seat,
            tick,
            economy,
            units,
            villagers,
            late_economy,
            scouts,
            led,
            army,
            buildings,
            enemies,
            center,
            home,
            queued,
            spent: [0; 4],
            hold_town_center: false,
            saving_castle: false,
            hold_military_training: false,
            draining: BTreeSet::new(),
            production_drain: false,
            busy: BTreeSet::new(),
            resources: Default::default(),
            spots: Default::default(),
            land_routes: BTreeMap::new(),
            land_queries: 0,
            land_query_limit: config.land_queries_per_decision,
            site_queries: 0,
            orders: 0,
            order_limit: config.orders_per_decision,
            centers,
            reserved_workers,
            lines: OnceCell::new(),
        }
    }

    pub fn settlement_distance(&mut self, tile: Tile) -> f64 {
        if let Some(known) = self.board.settlements.distances.get(&tile) {
            return *known;
        }
        let distance = if self.centers.is_empty() {
            self.home.map_or(1e9, |home| dist(tile, home))
        } else {
            self.centers
                .iter()
                .map(|center| dist(tile, *center))
                .fold(f64::INFINITY, f64::min)
        };
        self.board.settlements.distances.insert(tile, distance);
        distance
    }

    // ----- queries ----------------------------------------------------------
    pub fn feudal(&self) -> bool {
        self.economy
            .as_ref()
            .is_some_and(|e| matches!(e.age, Age::Feudal | Age::Castle | Age::Imperial))
    }

    /// Castle Age or later: every Castle capability stays in Imperial Age.
    pub fn castle(&self) -> bool {
        self.economy
            .as_ref()
            .is_some_and(|e| matches!(e.age, Age::Castle | Age::Imperial))
    }

    pub fn imperial(&self) -> bool {
        self.economy
            .as_ref()
            .is_some_and(|e| e.age == Age::Imperial)
    }

    pub fn count(&self, kind: BuildingKind, complete: bool) -> usize {
        self.buildings
            .iter()
            .filter(|b| b.kind == kind && (b.complete || !complete))
            .count()
    }

    pub fn resources(&mut self, kind: ResourceKind) -> &[Node] {
        let i = slot(kind);
        if self.resources[i].is_none() {
            let bad = &mut self.board.bad_resources;
            if bad.len() > 8192 {
                bad.clear(); // bounded: refused nodes are learned again if still bad
            }
            let nodes = self
                .world
                .resources(kind)
                .into_iter()
                .filter(|n| !bad.contains(&n.id))
                .collect();
            self.resources[i] = Some(nodes);
        }
        self.resources[i].as_deref().unwrap_or_default()
    }

    /// Tiles from a node to the nearest finished building that takes it.
    pub fn dropoff_distance(&mut self, node: &Node) -> f64 {
        let i = slot(node.kind);
        if self.spots[i].is_none() {
            let kinds = dropoff_kinds(node.kind);
            let spots: Vec<Tile> = self
                .buildings
                .iter()
                .filter(|b| b.complete && kinds.contains(&b.kind))
                .map(Building::center)
                .collect();
            // Distances are kept while the kind's sites stay the same.
            let stale = self.board.dropoffs[i]
                .as_ref()
                .is_none_or(|(held, known)| *held != spots || known.len() > DROPOFF_MEMO);
            if stale {
                self.board.dropoffs[i] = Some((spots.clone(), BTreeMap::new()));
            }
            self.spots[i] = Some(spots);
        }
        let spots = self.spots[i].as_deref().unwrap_or_default();
        if spots.is_empty() {
            return 1e9;
        }
        let Some((_, known)) = self.board.dropoffs[i].as_mut() else {
            return 1e9;
        };
        *known.entry(node.tile()).or_insert_with(|| {
            spots
                .iter()
                .map(|s| dist(node.tile(), *s))
                .fold(f64::INFINITY, f64::min)
        })
    }

    /// `tile`, or the nearest tile around it the server has not refused as a
    /// destination (a tree, water or a building: asking again changes nothing).
    pub fn unrefused(&self, tile: Tile) -> Tile {
        let bad = &self.board.bad_moves;
        if !bad.contains(&tile) {
            return tile;
        }
        let size = self.world.map_size();
        for radius in [2, 4, 6, 9, 13] {
            for (dx, dy) in [
                (1, 0),
                (0, 1),
                (-1, 0),
                (0, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
                (-1, -1),
            ] {
                let candidate = clamp_tile((tile.0 + dx * radius, tile.1 + dy * radius), size, 3);
                if !bad.contains(&candidate) {
                    return candidate;
                }
            }
        }
        tile
    }

    // ----- budget -----------------------------------------------------------
    pub fn combat_lines(&self) -> &Lines {
        self.lines.get_or_init(|| {
            let mut kinds = Lines::default();
            let soldiers = self.army.iter().chain(&self.led).filter(|u| u.alive);
            for unit in soldiers {
                kinds.add(unit.kind, 1);
            }
            for unit in self.units.iter().filter(|u| u.alive && warship(u.kind)) {
                kinds.add(unit.kind, 1);
            }
            for building in &self.buildings {
                if building.queue_length > 0 {
                    for kind in &building.queued_units {
                        if land_soldier(*kind) || warship(*kind) {
                            kinds.add(*kind, 1);
                        }
                    }
                }
                for (_, kind, hp, _) in &building.garrison {
                    if *hp > 0 && land_soldier(*kind) {
                        kinds.add(*kind, 1);
                    }
                }
            }
            let mut lines = Lines::default();
            for (kind, count) in kinds.iter() {
                lines.add(profile(kind).line, count);
            }
            lines
        })
    }

    pub fn can_afford(&self, cost: Cost) -> bool {
        let Some(e) = &self.economy else {
            return false;
        };
        let have = [e.food, e.wood, e.gold, e.stone];
        (0..4).all(|i| have[i] >= self.spent[i] && have[i] - self.spent[i] >= cost[i])
    }

    pub fn spend(&mut self, cost: Cost) -> Result<(), OrderBudgetExhausted> {
        self.require_order_budget()?;
        for (spent, price) in self.spent.iter_mut().zip(cost) {
            *spent += price;
        }
        Ok(())
    }

    /// Set aside at most what is left of the stock.
    pub fn reserve(&mut self, cost: Cost) {
        let Some(e) = &self.economy else {
            return;
        };
        let have = [e.food, e.wood, e.gold, e.stone];
        for i in 0..4 {
            self.spent[i] += cost[i].min(have[i].saturating_sub(self.spent[i]));
        }
    }

    pub fn try_spend(&mut self, cost: Cost) -> Result<bool, OrderBudgetExhausted> {
        if self.can_afford(cost) {
            self.spend(cost)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Room left in the population cap after what is queued.
    pub fn free_population(&self) -> i64 {
        self.economy.as_ref().map_or(0, |e| {
            i64::from(e.population_cap) - i64::from(e.population) - i64::from(self.queued)
        })
    }

    // ----- orders -----------------------------------------------------------
    pub fn require_order_budget(&self) -> Result<(), OrderBudgetExhausted> {
        if self.orders >= self.order_limit.min(self.config.orders_per_decision) {
            Err(OrderBudgetExhausted)
        } else {
            Ok(())
        }
    }

    /// Send an order and remember it until its verdict arrives. `None` when
    /// the order budget is spent: nothing is sent then.
    pub fn order(&mut self, name: &str, order: Order) -> Option<u64> {
        self.require_order_budget().ok()?;
        self.busy.extend(worked_units(&order));
        let sequence = self.seat.send(order.clone());
        let tracked = &mut self.board.tracked;
        tracked.insert(
            sequence,
            Tracked {
                name: name.to_owned(),
                order,
                tick: self.tick,
            },
        );
        if tracked.len() > 4096 {
            let stale: Vec<u64> = tracked.keys().take(1024).copied().collect();
            for sequence in stale {
                tracked.remove(&sequence);
            }
        }
        self.orders += 1;
        self.board.bump("orders");
        self.board.bump(&format!("order:{name}"));
        Some(sequence)
    }
}
