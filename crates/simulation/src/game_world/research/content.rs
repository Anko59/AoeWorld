//! Runtime presentation data projected from the authoritative simulation tables.
use crate::{Stockpile, UnitCombat, UnitCost, UnitKind, UnitTraining};

const MAGIC: i32 = 0x414f4543;
const BUILDING_WORDS: usize = 14;
const TECH_WORDS: usize = 12;
const UNIT_WORDS: usize = 20;

#[derive(Debug)]
pub struct BrowserBuilding {
    pub kind: u8,
    pub cost: Stockpile,
    pub side: i32,
    pub depth: i32,
    pub age: u8,
    pub buildable: bool,
    pub requires: Option<u8>,
    trainable: u32,
    pub blocks: bool,
    pub max_hp: u16,
    pub population_support: u16,
}
impl BrowserBuilding {
    pub fn trains(&self, kind: u8) -> bool {
        self.trainable & (1 << kind) != 0
    }
}

#[derive(Debug)]
pub struct BrowserTech {
    pub tech: u8,
    pub building: u8,
    pub age: u8,
    pub cost: Stockpile,
    pub requires_tech: Option<u8>,
    pub building_kinds: u32,
    pub building_count: u8,
    pub seconds: u32,
    pub capacity_permyriad: u32,
}

#[derive(Debug)]
pub struct BrowserUnit {
    pub combat: UnitCombat,
    pub age: u8,
    pub requires_tech: Option<u8>,
    upgrades: u64,
}
impl BrowserUnit {
    pub fn superseded(&self, researched: u64) -> bool {
        self.upgrades & researched != 0
    }
}

/// One current, bounded numeric asset. No filesystem, browser, or game state.
/// It is generated natively; production WASM only retains the decoder and data
/// loaded at startup. Counts/row identities reject stale or partial assets.
#[derive(Debug)]
pub struct BrowserContent {
    pub buildings: Vec<BrowserBuilding>,
    pub techs: Vec<BrowserTech>,
    pub units: Vec<BrowserUnit>,
    pub villager_food: u32,
}

fn cost(row: &[i32]) -> Stockpile {
    Stockpile {
        food: row[0] as u32,
        wood: row[1] as u32,
        gold: row[2] as u32,
        stone: row[3] as u32,
    }
}
fn optional(value: i32) -> Option<u8> {
    (value > 0).then_some(value.wrapping_sub(1) as u8)
}

impl BrowserContent {
    pub fn decode(data: &[i32]) -> Result<Self, &'static str> {
        let buildings = crate::BuildingKind::StoneGate as usize + 1;
        let techs = crate::Tech::WarGalley as usize + 1;
        let units = UnitKind::ALL.len();
        let length = 5 + buildings * BUILDING_WORDS + techs * TECH_WORDS + units * UNIT_WORDS;
        if data.len() != length
            || data[..4] != [MAGIC, buildings as i32, techs as i32, units as i32]
            || !(1..=65535).contains(&data[4])
        {
            return Err("invalid game content header");
        }
        // All words are bounded before constructing records. Only melee armour
        // and the low upgrade mask are signed. No attacker-selected allocation.
        for (index, &word) in data[5..].iter().enumerate() {
            let unit_start = buildings * BUILDING_WORDS + techs * TECH_WORDS;
            let signed = index >= unit_start && matches!((index - unit_start) % UNIT_WORDS, 4 | 18);
            let training_mask = index < buildings * BUILDING_WORDS && index % BUILDING_WORDS == 10;
            let valid = if training_mask {
                word >= 0 && word < (1 << units)
            } else {
                signed || (0..=1_000_000).contains(&word)
            };
            if !valid {
                return Err("invalid game content word");
            }
        }
        let mut offset = 5;
        let mut result = Self {
            buildings: Vec::with_capacity(buildings),
            techs: Vec::with_capacity(techs),
            units: Vec::with_capacity(units),
            villager_food: data[4] as u32,
        };
        for kind in 0..buildings {
            let r = &data[offset..offset + BUILDING_WORDS];
            offset += BUILDING_WORDS;
            if r[0] != kind as i32
                || !(1..=8).contains(&r[5])
                || !(1..=8).contains(&r[6])
                || r[7] > 2
                || r[8] > 1
                || r[9] > buildings as i32
                || r[10] >= 1 << units
                || r[11] > 1
                || r[12] > 65535
                || r[13] > 65535
            {
                return Err("invalid game content building");
            }
            result.buildings.push(BrowserBuilding {
                kind: r[0] as u8,
                cost: cost(&r[1..5]),
                side: r[5],
                depth: r[6],
                age: r[7] as u8,
                buildable: r[8] != 0,
                requires: optional(r[9]),
                trainable: r[10] as u32,
                blocks: r[11] != 0,
                max_hp: r[12] as u16,
                population_support: r[13] as u16,
            });
        }
        for tech in 0..techs {
            let r = &data[offset..offset + TECH_WORDS];
            offset += TECH_WORDS;
            if r[0] != tech as i32
                || r[1] >= buildings as i32
                || r[2] > 2
                || r[7] > techs as i32
                || r[8] >= 1 << buildings
                || r[9] > buildings as i32
            {
                return Err("invalid game content technology");
            }
            result.techs.push(BrowserTech {
                tech: r[0] as u8,
                building: r[1] as u8,
                age: r[2] as u8,
                cost: cost(&r[3..7]),
                requires_tech: optional(r[7]),
                building_kinds: r[8] as u32,
                building_count: r[9] as u8,
                seconds: r[10] as u32,
                capacity_permyriad: r[11] as u32,
            });
        }
        for kind in UnitKind::ALL {
            let r = &data[offset..offset + UNIT_WORDS];
            offset += UNIT_WORDS;
            if r[0] != kind as i32
                || r[1] > 65535
                || r[2] > 65535
                || r[3] > 1
                || !(-32768..=32767).contains(&r[4])
                || r[5] > 65535
                || r[8] > 255
                || r[9] > 255
                || r[10] > 255
                || r[16] > 2
                || r[17] > techs as i32
                || r[19] >= 1 << (techs - 32)
            {
                return Err("invalid game content unit");
            }
            let c = cost(&r[11..15]);
            result.units.push(BrowserUnit {
                combat: UnitCombat {
                    kind,
                    hp: r[1] as u16,
                    attack: r[2] as u16,
                    attack_kind: if r[3] == 0 {
                        crate::DamageKind::Melee
                    } else {
                        crate::DamageKind::Pierce
                    },
                    melee_armour: r[4] as i16,
                    pierce_armour: r[5] as u16,
                    attack_period_centiseconds: r[6] as u32,
                    speed_hundredths: (r[7] != 0).then_some(r[7] as u32),
                    range: r[8] as u8,
                    min_range: r[9] as u8,
                    los: r[10] as u8,
                    training: (r[15] != 0).then_some(UnitTraining {
                        cost: UnitCost {
                            food: c.food,
                            wood: c.wood,
                            gold: c.gold,
                            stone: c.stone,
                        },
                        seconds: r[15] as u32,
                    }),
                },
                age: r[16] as u8,
                requires_tech: optional(r[17]),
                upgrades: u64::from(r[18] as u32) | (u64::from(r[19] as u32) << 32),
            });
        }
        Ok(result)
    }

    pub fn villager_capacity(&self, researched: u64) -> u16 {
        self.techs
            .iter()
            .filter(|r| researched & (1 << r.tech) != 0 && r.capacity_permyriad > 0)
            .fold(10_000_u32, |total, r| {
                total.saturating_mul(r.capacity_permyriad) / 10_000
            })
            .saturating_mul(u32::from(crate::VILLAGE_RULES.villager_capacity))
            .div_ceil(10_000) as u16
    }
}

#[cfg(any(not(target_arch = "wasm32"), feature = "content-source"))]
mod source;
#[cfg(any(not(target_arch = "wasm32"), feature = "content-source"))]
pub use source::browser_content;

#[cfg(test)]
mod tests;
