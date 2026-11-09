//! The owner's 100-seat late game (2026-11-06, 34 minutes, ticks
//! 131,673-151,193) as seat shapes: slot 0 a 1,650-pop Imperial human with 25
//! Town Centers, 16 Castles, 220 stone wall tiles, 12 bridges and an
//! 870-soldier army; the AIs mostly Castle Age, about 27 villagers and 50
//! soldiers each. Shared by the server's `start_units.late_game` and the
//! in-process match benchmark (`aoe-harness match-bench`).
use super::LateGameSeat;
use crate::{Age, BuildingKind as B, UnitKind as U};

impl LateGameSeat {
    /// Slot 0: the owner's own base and army.
    pub fn owner_human() -> Self {
        Self {
            age: Age::Imperial,
            villagers: 728,
            fishing_ships: 51,
            army: vec![
                (U::Champion, 314),
                (U::Arbalester, 230),
                (U::Paladin, 142),
                (U::EliteSkirmisher, 87),
                (U::Hussar, 63),
                (U::Halberdier, 20),
            ],
            buildings: vec![
                (B::TownCenter, 24),
                (B::Castle, 16),
                (B::Mill, 54),
                (B::LumberCamp, 69),
                (B::MiningCamp, 13),
                (B::Barracks, 14),
                (B::ArcheryRange, 15),
                (B::Stable, 24),
                (B::WatchTower, 19),
                (B::Blacksmith, 1),
                (B::University, 1),
                (B::Market, 1),
                (B::Farm, 381),
                (B::House, 259),
            ],
            wall_tiles: 220,
            bridges: 12,
            radius: 200,
            stockpile: 20_000,
            // Two hours had worked out the land around the main village.
            depleted_radius: 90,
        }
    }

    /// An AI seat: the owner's AIs were 69 Castle, 17 Feudal, 10 Imperial and
    /// 4 Dark of 100; here every tenth seat is Imperial, every sixth Feudal.
    pub fn owner_ai(seat: usize) -> Self {
        let age = if seat.is_multiple_of(10) {
            Age::Imperial
        } else if seat.is_multiple_of(6) {
            Age::Feudal
        } else {
            Age::Castle
        };
        Self {
            age,
            villagers: 27,
            fishing_ships: 1,
            army: vec![
                (U::Pikeman, 14),
                (U::Crossbowman, 13),
                (U::Skirmisher, 11),
                (U::Spearman, 7),
                (U::Knight, 2),
                (U::Halberdier, 1),
                (U::Galley, 1),
            ],
            buildings: vec![
                (B::Mill, 5),
                (B::LumberCamp, 5),
                (B::MiningCamp, 1),
                (B::Barracks, 1),
                (B::ArcheryRange, 1),
                (B::Stable, 1),
                (B::Blacksmith, 1),
                (B::Farm, 3),
                (B::House, 19),
            ],
            wall_tiles: 0,
            bridges: 0,
            radius: 60,
            // The owner's AIs held little (one had 168 food, 17,173 wood): a
            // large stockpile let the bots train to 14,000 units in 12 minutes.
            stockpile: 1_500,
            depleted_radius: 30,
        }
    }

    /// The seat with `factor` times its units, farms, houses and camps on
    /// `sqrt(factor)` times the radius (the same density). Singletons
    /// (Blacksmith, University, Market), Town Centers, Castles, walls and
    /// bridges stay as they are. `factor` 1 returns the seat unchanged.
    pub fn scaled(&self, factor: f64) -> Self {
        let times = |n: u32| (f64::from(n) * factor).round().max(0.0) as u32;
        let fixed = |kind: B| {
            matches!(
                kind,
                B::Blacksmith | B::University | B::Market | B::TownCenter | B::Castle
            )
        };
        Self {
            age: self.age,
            villagers: times(self.villagers),
            fishing_ships: times(self.fishing_ships),
            army: self.army.iter().map(|&(k, n)| (k, times(n))).collect(),
            buildings: self
                .buildings
                .iter()
                .map(|&(k, n)| (k, if fixed(k) { n } else { times(n) }))
                .collect(),
            wall_tiles: self.wall_tiles,
            bridges: self.bridges,
            radius: (f64::from(self.radius) * factor.max(0.0).sqrt()).round() as i32,
            stockpile: self.stockpile,
            depleted_radius: (f64::from(self.depleted_radius) * factor.max(0.0).sqrt()).round()
                as i32,
        }
    }

    /// Units the seat places at most.
    pub fn unit_target(&self) -> u64 {
        u64::from(self.villagers + self.fishing_ships)
            + self.army.iter().map(|(_, n)| u64::from(*n)).sum::<u64>()
    }
}
