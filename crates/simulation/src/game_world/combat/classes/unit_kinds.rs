//! Unit-kind class predicates shared by combat, research and training.
use crate::UnitKind;

impl UnitKind {
    /// Battering Ram and its two upgrades.
    pub const fn is_ram(self) -> bool {
        matches!(self, Self::BatteringRam | Self::CappedRam | Self::SiegeRam)
    }
    /// Stone throwers and cannons: their projectile lands at a fixed point.
    pub const fn is_artillery(self) -> bool {
        matches!(
            self,
            Self::Mangonel
                | Self::Onager
                | Self::SiegeOnager
                | Self::BombardCannon
                | Self::Trebuchet
                | Self::TrebuchetUnpacked
        )
    }
    /// Scorpions fire bolts that pass through every enemy on their line.
    pub const fn is_scorpion(self) -> bool {
        matches!(self, Self::Scorpion | Self::HeavyScorpion)
    }
    pub const fn is_trebuchet(self) -> bool {
        matches!(self, Self::Trebuchet | Self::TrebuchetUnpacked)
    }
    pub const fn is_siege(self) -> bool {
        self.is_ram() || self.is_artillery() || self.is_scorpion() || matches!(self, Self::Petard)
    }
    /// Mounted units: the Stable's lines and the Cavalry Archer line.
    pub const fn is_mounted(self) -> bool {
        matches!(
            self,
            Self::Scout
                | Self::Knight
                | Self::LightCavalry
                | Self::Cavalier
                | Self::Paladin
                | Self::Hussar
                | Self::CamelRider
                | Self::HeavyCamelRider
                | Self::CavalryArcher
                | Self::HeavyCavalryArcher
        )
    }
    /// The three demolition hulls, which detonate on contact.
    pub const fn is_demolition(self) -> bool {
        matches!(
            self,
            Self::DemolitionRaft | Self::DemolitionShip | Self::HeavyDemolitionShip
        )
    }
    /// Cannon and Catapult Galleons throw splash shot like a land catapult.
    pub const fn is_cannon_ship(self) -> bool {
        matches!(
            self,
            Self::CannonGalleon | Self::EliteCannonGalleon | Self::CatapultGalleon
        )
    }
    /// Galley, War Galley and Galleon: the ranged pierce line.
    pub const fn is_galley_line(self) -> bool {
        matches!(self, Self::Galley | Self::WarGalley | Self::Galleon)
    }
    /// Fire Galley, Fire Ship and Fast Fire Ship.
    pub const fn is_fire_line(self) -> bool {
        matches!(self, Self::FireGalley | Self::FireShip | Self::FastFireShip)
    }
    /// Hulk, War Hulk and Carrack: the melee-armoured line.
    pub const fn is_hulk_line(self) -> bool {
        matches!(self, Self::Hulk | Self::WarHulk | Self::Carrack)
    }
    pub const fn is_naval(self) -> bool {
        matches!(
            self,
            Self::FishingShip
                | Self::Galley
                | Self::FireShip
                | Self::DemolitionShip
                | Self::TransportShip
                | Self::WarGalley
                | Self::TradeCog
                | Self::FireGalley
                | Self::FastFireShip
                | Self::DemolitionRaft
                | Self::HeavyDemolitionShip
                | Self::Galleon
                | Self::CannonGalleon
                | Self::EliteCannonGalleon
                | Self::Hulk
                | Self::WarHulk
                | Self::Carrack
                | Self::CatapultGalleon
        )
    }
}
