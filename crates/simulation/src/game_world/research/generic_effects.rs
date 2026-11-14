//! Effects of the generic technologies (cost/age data in `generic.rs`; the
//! mechanics and their limits are listed in docs/economy/generic-techs.md).
//! Every function takes the player's `researched` mask so hot paths (movement,
//! spawning, combat) never look a technology up through the world.
use super::{TECH_RULES, Tech, TechEffect, TechMask, TechTarget};
use crate::{Age, BuildingKind, UnitKind};

impl UnitKind {
    /// The militia and spear lines; Fire Lancers and Varangian Guards join them
    /// when those units exist.
    pub const fn is_infantry(self) -> bool {
        matches!(
            self,
            Self::Militia
                | Self::ManAtArms
                | Self::Spearman
                | Self::LongSwordsman
                | Self::Pikeman
                | Self::TwoHandedSwordsman
                | Self::Champion
                | Self::Halberdier
        )
    }
    /// The militia line alone: Gambesons.
    pub const fn is_militia_line(self) -> bool {
        matches!(
            self,
            Self::Militia
                | Self::ManAtArms
                | Self::LongSwordsman
                | Self::TwoHandedSwordsman
                | Self::Champion
        )
    }
    /// Archers and skirmishers: Thumb Ring.
    pub const fn is_archery(self) -> bool {
        matches!(
            self,
            Self::Archer
                | Self::Skirmisher
                | Self::Crossbowman
                | Self::EliteSkirmisher
                | Self::Arbalester
        )
    }
    /// Mounted archers (Parthian Tactics). Content track A appends them here.
    pub const fn is_mounted_archer(self) -> bool {
        false
    }
    /// Cavalry and mounted archers: Husbandry, Bloodlines.
    pub fn is_mounted(self) -> bool {
        self.is_cavalry() || self.is_mounted_archer()
    }
}

impl TechTarget {
    pub fn includes(self, kind: UnitKind) -> bool {
        match self {
            Self::Infantry => kind.is_infantry(),
            Self::Mounted => kind.is_mounted(),
            Self::Monk => kind == UnitKind::Monk,
            Self::Trade => matches!(kind, UnitKind::TradeCart | UnitKind::TradeCog),
        }
    }
}

type SpeedRows = Vec<(TechMask, TechTarget, u16)>;

fn speed_rows() -> &'static SpeedRows {
    static ROWS: std::sync::OnceLock<SpeedRows> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        TECH_RULES
            .iter()
            .filter_map(|r| match r.effect {
                TechEffect::UnitSpeed { target, percent } => Some((r.tech.bit(), target, percent)),
                _ => None,
            })
            .collect()
    })
}

/// Walking speed, in percent of the unit's base speed, with these technologies.
pub(crate) fn unit_speed_percent(kind: UnitKind, researched: TechMask) -> u16 {
    speed_rows()
        .iter()
        .filter(|(bit, target, _)| researched & bit != 0 && target.includes(kind))
        .fold(100, |total, (_, _, percent)| {
            (u32::from(total) * u32::from(*percent) / 100) as u16
        })
}

/// Trade units' speed with these technologies (Caravan). Trade units arrive
/// with the trade track, which multiplies its base speed by this.
pub fn trade_speed_percent(researched: TechMask) -> u16 {
    speed_rows()
        .iter()
        .filter(|(bit, target, _)| researched & bit != 0 && *target == TechTarget::Trade)
        .fold(100, |total, (_, _, percent)| {
            (u32::from(total) * u32::from(*percent) / 100) as u16
        })
}

/// Hit points added to every unit of `kind` by these technologies
/// (Bloodlines, Sanctity).
pub(crate) fn unit_hp_bonus(kind: UnitKind, researched: TechMask) -> u16 {
    if researched == 0 {
        return 0;
    }
    TECH_RULES
        .iter()
        .filter(|r| researched & r.tech.bit() != 0)
        .map(|r| match r.effect {
            TechEffect::UnitHp { target, hp } if target.includes(kind) => hp,
            _ => 0,
        })
        .sum()
}

const fn is_masonry_kind(kind: BuildingKind) -> bool {
    !matches!(kind, BuildingKind::Farm) && !kind.is_barrier()
}

/// Maximum hit points of a finished building. Masonry and Architecture each add
/// a tenth of the Age's value; the tower, wall and Castle upgrades add the
/// difference of the DAT records (Watch Tower 850 -> Guard Tower 1500 -> Keep
/// 2250, Stone Wall 1080 -> Fortified Wall 3000; Hoardings +1000).
pub fn building_max_hp(kind: BuildingKind, age: Age, researched: TechMask) -> u16 {
    let base = kind.age_stats(age).0;
    let has = |t: Tech| researched & t.bit() != 0;
    let mut hp = base;
    if is_masonry_kind(kind) {
        hp += base / 10 * u16::from(has(Tech::Masonry));
        hp += base / 10 * u16::from(has(Tech::Architecture));
    }
    hp.saturating_add(match kind {
        BuildingKind::WatchTower if has(Tech::Keep) => 1400,
        BuildingKind::WatchTower if has(Tech::GuardTower) => 650,
        BuildingKind::StoneWall if has(Tech::FortifiedWall) => 1920,
        BuildingKind::Castle if has(Tech::Hoardings) => 1000,
        _ => 0,
    })
}

/// Armour added to a building's base armour by these technologies (Masonry's
/// own point is applied by the combat code): Architecture +1/+1, Guard Tower
/// +1/+1, Keep +2/+2 over the Watch Tower, Fortified Wall +4/+2.
pub(crate) fn building_armour_bonus(kind: BuildingKind, researched: TechMask) -> (i16, u16) {
    if researched == 0 {
        return (0, 0);
    }
    let has = |t: Tech| researched & t.bit() != 0;
    let mut bonus = (0, 0);
    if is_masonry_kind(kind) && has(Tech::Architecture) {
        bonus = (1, 1);
    }
    let (melee, pierce) = match kind {
        BuildingKind::WatchTower if has(Tech::Keep) => (2, 2),
        BuildingKind::WatchTower if has(Tech::GuardTower) => (1, 1),
        BuildingKind::StoneWall if has(Tech::FortifiedWall) => (4, 2),
        _ => (0, 0),
    };
    (bonus.0 + melee, bonus.1 + pierce)
}

/// A tower's arrow attack: Guard Tower 7 and Keep 8 over the Watch Tower's 5,
/// then Arrowslits (+1 Watch Tower, +2 Guard Tower, +3 Keep).
pub(crate) fn tower_attack(kind: BuildingKind, base: u16, researched: TechMask) -> u16 {
    if kind != BuildingKind::WatchTower || researched == 0 {
        return base;
    }
    let has = |t: Tech| researched & t.bit() != 0;
    let (upgrade, slits) = if has(Tech::Keep) {
        (3, 3)
    } else if has(Tech::GuardTower) {
        (2, 2)
    } else {
        (0, 1)
    };
    base + upgrade + if has(Tech::Arrowslits) { slits } else { 0 }
}

/// Percent of the normal construction rate: Treadmill Crane.
pub fn build_speed_percent(researched: TechMask) -> u64 {
    if researched & Tech::TreadmillCrane.bit() != 0 {
        120
    } else {
        100
    }
}

/// Percent of the normal training rate of a military building: Conscription.
/// Siege Workshops are excluded.
pub fn training_speed_percent(building: BuildingKind, researched: TechMask) -> u64 {
    if researched & Tech::Conscription.bit() != 0
        && matches!(
            building,
            BuildingKind::Barracks
                | BuildingKind::ArcheryRange
                | BuildingKind::Stable
                | BuildingKind::Castle
        )
    {
        133
    } else {
        100
    }
}

/// Market commission in ten-thousandths: 30%, 15% with Guilds.
pub fn market_commission_permyriad(base: u16, researched: TechMask) -> u16 {
    if researched & Tech::Guilds.bit() != 0 {
        base / 2
    } else {
        base
    }
}

/// Tribute fee in ten-thousandths: 30%, 20% with Coinage, none with Banking.
/// Tribute is not a game command yet; this is the rule the command will use.
pub fn tribute_fee_permyriad(researched: TechMask) -> u16 {
    if researched & Tech::Banking.bit() != 0 {
        0
    } else if researched & Tech::Coinage.bit() != 0 {
        2000
    } else {
        3000
    }
}

impl crate::GameWorld {
    /// Brings every building of `player` to `building_max_hp` for the player's
    /// current technologies, keeping damage already taken: the hit points
    /// gained (earned in proportion to the work done on a foundation) are added.
    pub(super) fn refresh_building_hp(&mut self, player: aoe_core::PlayerId) {
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        for b in self
            .economy
            .buildings
            .values_mut()
            .filter(|b| b.player == player)
        {
            let target = building_max_hp(b.kind, b.age, researched);
            if target <= b.max_hp {
                continue;
            }
            let bonus = target - b.max_hp;
            self.vision.dirty_buildings.insert(b.id);
            b.max_hp = target;
            b.hp += if b.is_complete() {
                bonus
            } else {
                (u64::from(bonus) * b.work_done / b.work_total.max(1)) as u16
            };
        }
    }
}

impl crate::GameWorld {
    /// Attack bonus a technology adds to one blow of `kind` (owned by `player`)
    /// on `target`, on top of the class `class_bonus`: Arson (infantry +2 vs
    /// buildings), Sappers (villagers +15 vs buildings, +3 vs rams), Siege
    /// Engineers (siege +20% of its bonus vs buildings), Parthian Tactics
    /// (mounted archers +2 vs the spear line).
    pub(crate) fn tech_attack_bonus(
        &self,
        kind: UnitKind,
        player: aoe_core::PlayerId,
        target: aoe_core::EntityId,
        class_bonus: u16,
    ) -> u16 {
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        if researched == 0 {
            return 0;
        }
        let has = |t: Tech| researched & t.bit() != 0;
        if self.building(target).is_some() {
            let mut bonus = 0;
            if kind.is_infantry() && has(Tech::Arson) {
                bonus += 2;
            }
            if kind == UnitKind::Villager && has(Tech::Sappers) {
                bonus += 15;
            }
            if kind.is_siege() && has(Tech::SiegeEngineers) {
                bonus += class_bonus / 5;
            }
            return bonus;
        }
        let Some(victim) = self.unit(target).map(|u| u.kind) else {
            return 0;
        };
        let mut bonus = 0;
        if kind == UnitKind::Villager && victim.is_ram() && has(Tech::Sappers) {
            bonus += 3;
        }
        if kind.is_mounted_archer()
            && matches!(
                victim,
                UnitKind::Spearman | UnitKind::Pikeman | UnitKind::Halberdier
            )
            && has(Tech::ParthianTactics)
        {
            bonus += 2;
        }
        bonus
    }

    /// Heated Shot: Towers +125% attack and Castles +4 attack against ships.
    pub(crate) fn heated_shot_bonus(
        &self,
        player: aoe_core::PlayerId,
        tower: BuildingKind,
        attack: u16,
        target: aoe_core::EntityId,
    ) -> u16 {
        if !self.has_researched(player, Tech::HeatedShot)
            || !self.unit(target).is_some_and(|u| u.kind.is_naval())
        {
            return 0;
        }
        match tower {
            BuildingKind::WatchTower => attack * 5 / 4,
            BuildingKind::Castle | BuildingKind::Dock => 4,
            _ => 0,
        }
    }
}

impl crate::GameWorld {
    /// Conscription: queued military training finishes 33% sooner; the share
    /// already done is kept.
    pub(super) fn speed_up_training(&mut self, player: aoe_core::PlayerId) {
        for b in self.economy.buildings.values_mut().filter(|b| {
            b.player == player && training_speed_percent(b.kind, Tech::Conscription.bit()) != 100
        }) {
            for entry in &mut b.training_queue {
                entry.progress_ticks = entry.progress_ticks * 100 / 133;
                entry.total_ticks = (entry.total_ticks * 100 / 133).max(1);
            }
        }
    }

    /// Treadmill Crane: foundations take a sixth less work (rate x1.2); the
    /// share already built is kept. Foundations placed later start smaller.
    pub(super) fn speed_up_construction(&mut self, player: aoe_core::PlayerId) {
        for b in self
            .economy
            .buildings
            .values_mut()
            .filter(|b| b.player == player && !b.is_complete())
        {
            b.work_done = b.work_done * 100 / 120;
            b.work_total = (b.work_total * 100 / 120).max(1);
        }
    }
}
