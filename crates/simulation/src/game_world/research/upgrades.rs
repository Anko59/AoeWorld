use crate::{BuildingKind, GameWorld, Tech, UnitKind};
use aoe_core::{EntityId, PlayerId};

impl UnitKind {
    pub const ALL: [Self; 24] = [
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
    ];
    /// The single current identity of each production line.
    pub fn upgraded(self, researched: u64) -> Self {
        let has = |t: Tech| researched & t.bit() != 0;
        match self {
            Self::Galley if has(Tech::WarGalley) => Self::WarGalley,
            Self::Militia | Self::ManAtArms if has(Tech::LongSwordsman) => Self::LongSwordsman,
            Self::Militia if has(Tech::ManAtArms) => Self::ManAtArms,
            Self::Spearman if has(Tech::Pikeman) => Self::Pikeman,
            Self::Archer if has(Tech::Crossbowman) => Self::Crossbowman,
            Self::Skirmisher if has(Tech::EliteSkirmisher) => Self::EliteSkirmisher,
            Self::Scout if has(Tech::LightCavalry) => Self::LightCavalry,
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
            _ => None,
        }
    }
    pub fn required_age(self) -> crate::Age {
        if matches!(self, Self::Galley | Self::FireShip | Self::DemolitionShip) {
            crate::Age::Feudal
        } else if self == Self::TransportShip {
            crate::Age::Dark
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
        let bonus = crate::unit_combat(to)
            .hp
            .saturating_sub(crate::unit_combat(from).hp);
        let mut vision = Vec::new();
        for u in &mut self.units {
            let s = &mut u.state;
            if s.player == player && s.kind == from && s.hp > 0 {
                s.kind = to;
                s.hp += bonus;
                s.max_hp += bonus;
                changed.push(s.id);
                vision.push(s.id);
            }
        }
        for b in self
            .economy
            .buildings
            .values_mut()
            .filter(|b| b.player == player)
        {
            for e in &mut b.training_queue {
                if e.kind == from {
                    e.kind = to;
                    if let Some(training) = crate::unit_combat(to).training {
                        let total = u64::from(training.seconds) * u64::from(self.config.tick_hz);
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
    pub(super) fn apply_masonry(&mut self, player: PlayerId) {
        for b in self.economy.buildings.values_mut().filter(|b| {
            b.player == player
                && !matches!(
                    b.kind,
                    BuildingKind::Farm | BuildingKind::PalisadeWall | BuildingKind::PalisadeGate
                )
        }) {
            let bonus = b.kind.age_stats(b.age).0 / 10;
            b.max_hp += bonus;
            b.hp += if b.is_complete() {
                bonus
            } else {
                (u64::from(bonus) * b.work_done / b.work_total.max(1)) as u16
            };
        }
    }
}
