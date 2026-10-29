//! DE class components, compiled into allocation-free attacker/victim tables.
//! Base melee/pierce components use the researched tables at impact.
//! Sources and the complete roster: docs/gameplay/hidden-bonuses.md.
use crate::{BuildingKind, GameWorld, Tech, UnitKind};
use aoe_core::EntityId;

mod profiles;
use profiles::{Class, building_armour, building_attacks, unit_armour, unit_attacks};

const UNITS: usize = super::UNIT_COMBAT.len();
const KINDS: usize = UNITS + super::BUILDING_COMBAT.len();
static BONUSES: [[[u16; KINDS]; KINDS]; 2] = bonus_tables();

const fn bonus_tables() -> [[[u16; KINDS]; KINDS]; 2] {
    let mut tables = [[[0; KINDS]; KINDS]; 2];
    let mut masonry = 0;
    while masonry < 2 {
        let mut a = 0;
        while a < KINDS {
            let attacks = if a < UNITS {
                unit_attacks(super::UNIT_COMBAT[a].kind)
            } else {
                building_attacks(super::BUILDING_COMBAT[a - UNITS].kind)
            };
            let mut v = 0;
            while v < KINDS {
                let armour = if v < UNITS {
                    unit_armour(super::UNIT_COMBAT[v].kind)
                } else {
                    building_armour(super::BUILDING_COMBAT[v - UNITS].kind)
                };
                let mut i = 0;
                while i < attacks.len() {
                    let mut j = 0;
                    while j < armour.len() {
                        if attacks[i].0 as u8 == armour[j].0 as u8 {
                            let extra = if masonry == 1
                                && v >= UNITS
                                && masonry_kind(super::BUILDING_COMBAT[v - UNITS].kind)
                                && armour[j].0 as u8 == Class::Building as u8
                            {
                                3
                            } else {
                                0
                            };
                            let damage = attacks[i].1 - armour[j].1 - extra;
                            if damage > 0 {
                                tables[masonry][a][v] += damage as u16;
                            }
                        }
                        j += 1;
                    }
                    i += 1;
                }
                v += 1;
            }
            a += 1;
        }
        masonry += 1;
    }
    tables
}

pub(super) const fn unit_bonus(attacker: UnitKind, victim: UnitKind) -> u16 {
    BONUSES[0][attacker as usize][victim as usize]
}

pub(super) const fn masonry_kind(kind: BuildingKind) -> bool {
    !matches!(
        kind,
        BuildingKind::Farm | BuildingKind::PalisadeWall | BuildingKind::PalisadeGate
    )
}

impl GameWorld {
    pub(in crate::game_world) fn unit_damage_bonus(&self, kind: UnitKind, target: EntityId) -> u16 {
        let bonus = self.unit(target).map_or_else(
            || self.building_class_bonus(kind as usize, target),
            |v| super::military::class_bonus(kind, v.kind),
        );
        if kind != UnitKind::FireShip {
            return bonus;
        }
        // Fire Ships have both pierce and melee attack components. The extra
        // melee component must see researched armour, independently of pierce.
        let melee = self.unit_armour(target).map(|a| a.0).or_else(|| {
            self.building(target).map(|b| {
                super::age_armour(b.kind, b.age).melee_armour
                    + i16::from(self.building_has_masonry(b))
            })
        });
        bonus + melee.map_or(0, |a| (1_i32 - i32::from(a)).max(0) as u16)
    }

    pub(super) fn building_damage_bonus(&self, kind: BuildingKind, target: EntityId) -> u16 {
        self.class_damage_bonus(UNITS + kind as usize, target)
    }

    fn class_damage_bonus(&self, attacker: usize, target: EntityId) -> u16 {
        if let Some(v) = self.unit(target) {
            return BONUSES[0][attacker][v.kind as usize];
        }
        self.building_class_bonus(attacker, target)
    }

    fn building_class_bonus(&self, attacker: usize, target: EntityId) -> u16 {
        self.building(target).map_or(0, |v| {
            BONUSES[usize::from(self.building_has_masonry(v))][attacker][UNITS + v.kind as usize]
        })
    }

    pub(super) fn building_has_masonry(&self, b: &crate::GameBuilding) -> bool {
        masonry_kind(b.kind) && self.has_researched(b.player, Tech::Masonry)
    }
}
