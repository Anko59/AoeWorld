use crate::{GameWorld, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId};

impl UnitKind {
    pub const ALL: [Self; 35] = [
        Self::Villager,
        Self::Scout,
        Self::Militia,
        Self::FishingShip,
        Self::Archer,
        Self::Skirmisher,
        Self::Spearman,
        Self::ManAtArms,
        Self::LongSwordsman,
        Self::Pikeman,
        Self::Crossbowman,
        Self::EliteSkirmisher,
        Self::Knight,
        Self::LightCavalry,
        Self::Monk,
        Self::Mangonel,
        Self::BatteringRam,
        Self::Galley,
        Self::FireShip,
        Self::DemolitionShip,
        Self::TransportShip,
        Self::WarGalley,
        Self::Vassal,
        Self::General,
        Self::TwoHandedSwordsman,
        Self::Champion,
        Self::Halberdier,
        Self::Arbalester,
        Self::Cavalier,
        Self::Paladin,
        Self::Hussar,
        Self::CappedRam,
        Self::SiegeRam,
        Self::Onager,
        Self::Steward,
    ];
    /// The single current identity of each production line.
    pub fn upgraded(self, researched: u128) -> Self {
        let has = |t: Tech| researched & t.bit() != 0;
        match self {
            Self::Galley if has(Tech::WarGalley) => Self::WarGalley,
            Self::Militia | Self::ManAtArms | Self::LongSwordsman | Self::TwoHandedSwordsman
                if has(Tech::Champion) =>
            {
                Self::Champion
            }
            Self::Militia | Self::ManAtArms | Self::LongSwordsman
                if has(Tech::TwoHandedSwordsman) =>
            {
                Self::TwoHandedSwordsman
            }
            Self::Militia | Self::ManAtArms if has(Tech::LongSwordsman) => Self::LongSwordsman,
            Self::Militia if has(Tech::ManAtArms) => Self::ManAtArms,
            Self::Spearman | Self::Pikeman if has(Tech::Halberdier) => Self::Halberdier,
            Self::Spearman if has(Tech::Pikeman) => Self::Pikeman,
            Self::Archer | Self::Crossbowman if has(Tech::Arbalester) => Self::Arbalester,
            Self::Archer if has(Tech::Crossbowman) => Self::Crossbowman,
            Self::Skirmisher if has(Tech::EliteSkirmisher) => Self::EliteSkirmisher,
            Self::Knight | Self::Cavalier if has(Tech::Paladin) => Self::Paladin,
            Self::Knight if has(Tech::Cavalier) => Self::Cavalier,
            Self::Scout | Self::LightCavalry if has(Tech::Hussar) => Self::Hussar,
            Self::Scout if has(Tech::LightCavalry) => Self::LightCavalry,
            Self::BatteringRam | Self::CappedRam if has(Tech::SiegeRam) => Self::SiegeRam,
            Self::BatteringRam if has(Tech::CappedRam) => Self::CappedRam,
            Self::Mangonel if has(Tech::Onager) => Self::Onager,
            _ => self,
        }
    }
    pub fn required_tech(self) -> Option<Tech> {
        match self {
            Self::WarGalley => Some(Tech::WarGalley),
            Self::ManAtArms => Some(Tech::ManAtArms),
            Self::LongSwordsman => Some(Tech::LongSwordsman),
            Self::Pikeman => Some(Tech::Pikeman),
            Self::Crossbowman => Some(Tech::Crossbowman),
            Self::EliteSkirmisher => Some(Tech::EliteSkirmisher),
            Self::LightCavalry => Some(Tech::LightCavalry),
            Self::TwoHandedSwordsman => Some(Tech::TwoHandedSwordsman),
            Self::Champion => Some(Tech::Champion),
            Self::Halberdier => Some(Tech::Halberdier),
            Self::Arbalester => Some(Tech::Arbalester),
            Self::Cavalier => Some(Tech::Cavalier),
            Self::Paladin => Some(Tech::Paladin),
            Self::Hussar => Some(Tech::Hussar),
            Self::CappedRam => Some(Tech::CappedRam),
            Self::SiegeRam => Some(Tech::SiegeRam),
            Self::Onager => Some(Tech::Onager),
            _ => None,
        }
    }
    pub fn required_age(self) -> crate::Age {
        if self == Self::Steward {
            crate::Age::Castle
        } else if matches!(self, Self::Galley | Self::FireShip | Self::DemolitionShip) {
            crate::Age::Feudal
        } else if self == Self::TransportShip {
            crate::Age::Dark
        } else if self as u8 >= Self::TwoHandedSwordsman as u8 {
            crate::Age::Imperial
        } else if self as u8 >= Self::LongSwordsman as u8 {
            crate::Age::Castle
        } else if matches!(self, Self::Villager | Self::Militia | Self::FishingShip) {
            crate::Age::Dark
        } else {
            crate::Age::Feudal
        }
    }
}
impl GameWorld {
    pub(super) fn upgrade_units(
        &mut self,
        player: PlayerId,
        from: UnitKind,
        to: UnitKind,
        changed: &mut Vec<EntityId>,
    ) {
        let mut upgraded = 0;
        let bonus = crate::unit_combat(to)
            .hp
            .saturating_sub(crate::unit_combat(from).hp);
        let mut vision = Vec::new();
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        for u in &mut self.units {
            let s = &mut u.state;
            if s.player == player && s.kind == from && s.hp > 0 {
                s.kind = to;
                upgraded += 1;
                s.hp += bonus;
                s.max_hp += bonus;
                changed.push(s.id);
                vision.push(s.id);
            }
        }
        self.change_unit_census(player, Some(from), Some(to), upgraded);
        for b in self
            .economy
            .buildings
            .values_mut()
            .filter(|b| b.player == player)
        {
            self.vision.dirty_buildings.insert(b.id);
            for e in &mut b.training_queue {
                if e.kind == from {
                    e.kind = to;
                    if let Some(training) = crate::unit_combat(to).training {
                        let total =
                            u64::from(training.seconds) * u64::from(self.config.tick_hz) * 100
                                / crate::training_speed_percent(b.kind, researched);
                        e.progress_ticks = e.progress_ticks * total / e.total_ticks.max(1);
                        e.total_ticks = total;
                    }
                }
            }
        }
        for id in vision {
            self.update_unit_vision(id);
        }
    }
}
