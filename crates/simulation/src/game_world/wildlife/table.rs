//! AoE II DE base values. Rates and speed are hundredths per game second.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AnimalKind {
    Sheep,
    Deer,
    Boar,
    Wolf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WildlifeRules {
    pub hp: u16,
    pub food: u16,
    pub attack: u16,
    pub speed_hundredths: u16,
    pub gather_hundredths: u16,
    pub decay_hundredths: u16,
    pub sight_tiles: u8,
    /// Fog LOS from unit data, separate from AI acquisition distance.
    pub los_tiles: u8,
    pub chase_tiles: u8,
    pub attack_period_centiseconds: u32,
}
pub const WILDLIFE_RULES: [WildlifeRules; 4] = [
    WildlifeRules {
        hp: 7,
        food: 100,
        attack: 0,
        speed_hundredths: 70,
        gather_hundredths: 33,
        decay_hundredths: 25,
        sight_tiles: 2,
        los_tiles: 3,
        chase_tiles: 0,
        attack_period_centiseconds: 0,
    },
    WildlifeRules {
        hp: 5,
        food: 140,
        attack: 0,
        speed_hundredths: 100,
        gather_hundredths: 41,
        decay_hundredths: 25,
        sight_tiles: 4,
        los_tiles: 2,
        chase_tiles: 0,
        attack_period_centiseconds: 0,
    },
    WildlifeRules {
        hp: 75,
        food: 340,
        attack: 7,
        speed_hundredths: 96,
        gather_hundredths: 41,
        decay_hundredths: 40,
        sight_tiles: 0,
        los_tiles: 4,
        chase_tiles: 8,
        attack_period_centiseconds: 200,
    },
    WildlifeRules {
        hp: 25,
        food: 0,
        attack: 3,
        speed_hundredths: 105,
        gather_hundredths: 0,
        decay_hundredths: 0,
        sight_tiles: 6,
        los_tiles: 12,
        chase_tiles: 0,
        attack_period_centiseconds: 200,
    },
];
impl AnimalKind {
    pub const fn rules(self) -> &'static WildlifeRules {
        &WILDLIFE_RULES[self as usize]
    }
}
