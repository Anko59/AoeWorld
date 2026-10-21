//! A deterministic stand-in for 99 AI controllers and one human: a fixed rate
//! of gather, move, attack, build and train orders spread over all players.
use super::world::Player;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_map::ResourceKind;
use aoe_simulation::{BuildingKind, GameWorld, UnitKind};
use std::collections::BTreeMap;

pub(super) const ORDER_KINDS: [&str; 6] = [
    "gather",
    "move_military",
    "attack_neighbour",
    "build_house",
    "train_villager",
    "move_villager",
];

#[derive(Default)]
pub(super) struct Outcomes {
    pub ok: BTreeMap<&'static str, u64>,
    pub rejected: BTreeMap<&'static str, BTreeMap<String, u64>>,
}

pub(super) struct Driver {
    random: u64,
    carry_millis: u64,
    rate_millis_per_tick: u64,
    pub outcomes: Outcomes,
    pub issued: u64,
    /// Drops attack orders (they would change outcome once far attacks succeed).
    pub skip_attacks: bool,
}

impl Driver {
    /// `orders_per_second` per player at `tick_hz` ticks per second.
    pub(super) fn new(seed: u64, players: usize, orders_per_second: f64, tick_hz: u32) -> Self {
        let per_tick = players as f64 * orders_per_second / f64::from(tick_hz);
        Self {
            random: seed.max(1) ^ 0x9E37_79B9_7F4A_7C15,
            carry_millis: 0,
            rate_millis_per_tick: (per_tick * 1000.0).round() as u64,
            outcomes: Outcomes::default(),
            issued: 0,
            skip_attacks: false,
        }
    }

    fn next(&mut self) -> u64 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        self.random
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[(self.next() % items.len() as u64) as usize])
    }

    fn record<E: std::fmt::Debug>(&mut self, kind: &'static str, result: Result<(), E>) {
        match result {
            Ok(()) => *self.outcomes.ok.entry(kind).or_default() += 1,
            Err(error) => {
                let name = format!("{error:?}");
                let name = name.split(['(', ' ']).next().unwrap_or("").to_owned();
                *self
                    .outcomes
                    .rejected
                    .entry(kind)
                    .or_default()
                    .entry(name)
                    .or_default() += 1;
            }
        }
    }

    /// Issues this tick's share of orders; returns how many were issued.
    pub(super) fn issue(&mut self, world: &mut GameWorld, players: &mut [Player]) -> u32 {
        self.carry_millis += self.rate_millis_per_tick;
        let mut count = 0;
        while self.carry_millis >= 1000 && !players.is_empty() {
            self.carry_millis -= 1000;
            count += 1;
            self.issued += 1;
            let index = (self.next() % players.len() as u64) as usize;
            let roll = self.next() % 100;
            match roll {
                0..=34 => self.gather(world, &players[index]),
                35..=59 => self.move_military(world, &players[index]),
                60..=69 => self.attack(world, players, index),
                70..=79 => self.build_house(world, &mut players[index]),
                80..=89 => self.train(world, &players[index]),
                _ => self.move_villager(world, &players[index]),
            }
        }
        count
    }

    fn near(&mut self, player: &Player, spread: i64) -> Option<WorldPosition> {
        let dx = (self.next() % (2 * spread as u64 + 1)) as i64 - spread;
        let dy = (self.next() % (2 * spread as u64 + 1)) as i64 - spread;
        WorldPosition::from_tile_center(TileCoord::new(
            (i64::from(player.center.x) + dx).max(1) as i32,
            (i64::from(player.center.y) + dy).max(1) as i32,
        ))
        .ok()
    }

    fn gather(&mut self, world: &mut GameWorld, player: &Player) {
        let kinds = [
            ResourceKind::Wood,
            ResourceKind::Food,
            ResourceKind::Gold,
            ResourceKind::Stone,
        ];
        let wanted = self.pick(&kinds).unwrap_or(ResourceKind::Wood);
        let nodes: Vec<u64> = player
            .nodes
            .iter()
            .filter(|(kind, _)| *kind == wanted)
            .map(|(_, id)| *id)
            .collect();
        if let (Some(villager), Some(node)) = (self.pick(&player.villagers), self.pick(&nodes)) {
            let result = world.issue_gather(player.id, villager, node);
            self.record("gather", result);
        }
    }

    fn move_military(&mut self, world: &mut GameWorld, player: &Player) {
        if let (Some(unit), Some(target)) = (self.pick(&player.military), self.near(player, 80)) {
            let result = world.issue_move(unit, target).map(|_| ());
            self.record("move_military", result);
        }
    }

    fn move_villager(&mut self, world: &mut GameWorld, player: &Player) {
        if let (Some(unit), Some(target)) = (self.pick(&player.villagers), self.near(player, 40)) {
            let result = world.issue_move(unit, target).map(|_| ());
            self.record("move_villager", result);
        }
    }

    fn attack(&mut self, world: &mut GameWorld, players: &[Player], index: usize) {
        if self.skip_attacks {
            return;
        }
        let enemy = &players[(index + 1) % players.len()];
        if let Some(unit) = self.pick(&players[index].military) {
            let result = world.issue_attack(players[index].id, unit, enemy.town_center);
            self.record("attack_neighbour", result);
        }
    }

    fn build_house(&mut self, world: &mut GameWorld, player: &mut Player) {
        let builders: Vec<EntityId> = (0..3)
            .filter_map(|_| self.pick(&player.villagers))
            .collect();
        if let Some(position) = self.near(player, 18) {
            let result = world
                .issue_build(
                    player.id,
                    &builders,
                    BuildingKind::House,
                    position.tile_floor(),
                )
                .map(|_| ());
            if result.is_ok() {
                player.houses += 1;
            }
            self.record("build_house", result);
        }
    }

    fn train(&mut self, world: &mut GameWorld, player: &Player) {
        let result = world.issue_train(player.id, player.town_center, UnitKind::Villager);
        self.record("train_villager", result);
    }
}
