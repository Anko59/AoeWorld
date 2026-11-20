//! A coastal match with production buildings on both domains and passive targets.
use super::super::kingdom::{clear_kingdom_plot, kingdom_building, kingdom_position};
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{CombatStance, EntityId, PlayerId, TileCoord, TileRect};

const SEATS: [PlayerId; 2] = [PlayerId(0), PlayerId(1)];
// The intervening identities can only be trained before later research
// supersedes them. Each tier starts from the same map and buildings.
const TIERS: [&[Tech]; 5] = [
    &[],
    &[
        Tech::ManAtArms,
        Tech::Pikeman,
        Tech::Crossbowman,
        Tech::EliteSkirmisher,
        Tech::LightCavalry,
        Tech::WarGalley,
        Tech::DemolitionShip,
    ],
    &[
        Tech::LongSwordsman,
        Tech::Halberdier,
        Tech::Arbalester,
        Tech::Cavalier,
        Tech::Hussar,
        Tech::CappedRam,
        Tech::Onager,
        Tech::HeavyWarships,
        Tech::HeavyDemolitionShip,
        Tech::EliteCannonGalleon,
        Tech::HeavyCavalryArcher,
        Tech::HeavyCamelRider,
        Tech::HeavyScorpion,
    ],
    &[
        Tech::TwoHandedSwordsman,
        Tech::Paladin,
        Tech::SiegeRam,
        Tech::SiegeOnager,
    ],
    &[Tech::Champion],
];

impl GameWorld {
    /// Advance the ordinary training clock quickly during automated browser
    /// coverage. Recordings can use the ordinary simulation clock.
    pub fn prepare_content_roster(
        &mut self,
        stage: usize,
        realtime: bool,
    ) -> Result<EntityId, EconomyError> {
        let tiers = TIERS.get(..=stage).ok_or(EconomyError::InvalidTarget)?;
        if !realtime {
            self.config.tick_hz = 1;
        }
        self.set_fog_disabled(true);
        let dock = (30..80)
            .flat_map(|y| (32..self.config.width_tiles - 20).map(move |x| TileCoord::new(x, y)))
            .find(|&at| {
                self.validate_building_placement(SEATS[0], BuildingKind::Dock, at)
                    .is_ok()
            })
            .ok_or(EconomyError::InvalidFootprint)?;
        let at = |dx, dy| TileCoord::new(dock.x + dx, dock.y + dy);
        clear_kingdom_plot(self, TileRect::from_xywh(dock.x - 32, dock.y - 8, 54, 34))?;
        let primary = self.spawn_kind(SEATS[0], UnitKind::Scout, kingdom_position(at(-10, 0))?)?;
        for (kind, x, y) in [
            (BuildingKind::TownCenter, -13, 3),
            (BuildingKind::Barracks, -19, 3),
            (BuildingKind::ArcheryRange, -19, 9),
            (BuildingKind::Stable, -13, 9),
            (BuildingKind::SiegeWorkshop, -7, 9),
            (BuildingKind::Castle, -8, 2),
            (BuildingKind::Monastery, -14, -3),
            (BuildingKind::Market, -8, -3),
            (BuildingKind::Dock, 0, 0),
        ] {
            kingdom_building(self, kind, at(x, y))?;
        }
        for x in (-28..=-7).step_by(3) {
            kingdom_building(self, BuildingKind::House, at(x, 21))?;
        }
        for tech in [
            Tech::FeudalAge,
            Tech::CastleAge,
            Tech::ImperialAge,
            Tech::Chemistry,
        ]
        .into_iter()
        .chain(tiers.iter().flat_map(|tier| tier.iter().copied()))
        {
            self.complete_research(SEATS[0], tech, &mut Vec::new());
        }
        let target = self.insert_building(
            SEATS[1],
            BuildingKind::House,
            TileRect::from_xywh(dock.x - 27, dock.y + 5, 2, 2),
            true,
        )?;
        let house = self
            .building_mut(target)
            .ok_or(EconomyError::UnknownEntity)?;
        house.hp = 60_000;
        house.max_hp = house.hp;
        // Keep stationary targets outside the recruits' initial attack range.
        for (kind, x, y) in [
            (UnitKind::TransportShip, 17, 0),
            (UnitKind::TradeCart, -28, 6),
        ] {
            let id = self.spawn_kind(SEATS[1], kind, kingdom_position(at(x, y))?)?;
            let state = &mut self.units[self.lookup[&id]].state;
            state.hp = 60_000;
            state.max_hp = state.hp;
            state.stance = CombatStance::NoAttack;
        }
        self.issue_stance(SEATS[0], &[primary], CombatStance::NoAttack)?;
        self.economy
            .players
            .get_mut(&SEATS[0])
            .ok_or(EconomyError::NotOwner)?
            .stockpile = Stockpile {
            food: 40_000,
            wood: 40_000,
            gold: 40_000,
            stone: 40_000,
        };
        Ok(primary)
    }
}

#[path = "content/tests.rs"]
#[cfg(test)]
mod tests;
