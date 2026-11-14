//! Generic (non-regional) technologies of the Barracks, Archery Range, Stable,
//! University, Monastery, Castle and Market. Cost, time, Age, building and
//! prerequisite come from the licensed DE DAT (tech ids in the comments, data
//! dated 2026-10-27, DE update 188329); effect text from the DE help strings.
use super::super::{BuildingKind, Stockpile};
use super::table::*;
const fn cost(food: u32, wood: u32, gold: u32, stone: u32) -> Stockpile {
    Stockpile {
        food,
        wood,
        gold,
        stone,
    }
}
const fn generic(
    tech: Tech,
    building: BuildingKind,
    age: Age,
    cost: Stockpile,
    seconds: u32,
    requires: TechRequirement,
    effect: TechEffect,
) -> TechRules {
    TechRules {
        age,
        tech,
        building,
        cost,
        seconds,
        requires,
        effect,
    }
}
pub(super) const RULES: [TechRules; 34] = [
    // DAT tech 215
    generic(
        Tech::Squires,
        BuildingKind::Barracks,
        Age::Castle,
        cost(100, 0, 0, 0),
        40,
        TechRequirement::None,
        TechEffect::UnitSpeed {
            target: TechTarget::Infantry,
            percent: 110,
        },
    ),
    // DAT tech 602
    generic(
        Tech::Arson,
        BuildingKind::Barracks,
        Age::Feudal,
        cost(75, 0, 25, 0),
        25,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 875
    generic(
        Tech::Gambesons,
        BuildingKind::Barracks,
        Age::Castle,
        cost(50, 0, 100, 0),
        25,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 437
    generic(
        Tech::ThumbRing,
        BuildingKind::ArcheryRange,
        Age::Castle,
        cost(300, 250, 0, 0),
        45,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 436
    generic(
        Tech::ParthianTactics,
        BuildingKind::ArcheryRange,
        Age::Imperial,
        cost(200, 0, 250, 0),
        65,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 435
    generic(
        Tech::Bloodlines,
        BuildingKind::Stable,
        Age::Feudal,
        cost(150, 0, 100, 0),
        50,
        TechRequirement::None,
        TechEffect::UnitHp {
            target: TechTarget::Mounted,
            hp: 20,
        },
    ),
    // DAT tech 39
    generic(
        Tech::Husbandry,
        BuildingKind::Stable,
        Age::Castle,
        cost(150, 0, 0, 0),
        40,
        TechRequirement::None,
        TechEffect::UnitSpeed {
            target: TechTarget::Mounted,
            percent: 110,
        },
    ),
    // DAT tech 51
    generic(
        Tech::Architecture,
        BuildingKind::University,
        Age::Imperial,
        cost(300, 200, 0, 0),
        70,
        TechRequirement::Technology(Tech::Masonry),
        TechEffect::BuildingStats,
    ),
    // DAT tech 377
    generic(
        Tech::SiegeEngineers,
        BuildingKind::University,
        Age::Imperial,
        cost(500, 600, 0, 0),
        45,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 380
    generic(
        Tech::HeatedShot,
        BuildingKind::University,
        Age::Castle,
        cost(350, 0, 100, 0),
        30,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 608
    generic(
        Tech::Arrowslits,
        BuildingKind::University,
        Age::Imperial,
        cost(250, 250, 0, 0),
        25,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 194
    generic(
        Tech::FortifiedWall,
        BuildingKind::University,
        Age::Castle,
        cost(200, 100, 0, 0),
        50,
        TechRequirement::None,
        TechEffect::BuildingStats,
    ),
    // DAT tech 140
    generic(
        Tech::GuardTower,
        BuildingKind::University,
        Age::Castle,
        cost(100, 250, 0, 0),
        30,
        TechRequirement::None,
        TechEffect::BuildingStats,
    ),
    // DAT tech 63
    generic(
        Tech::Keep,
        BuildingKind::University,
        Age::Imperial,
        cost(500, 350, 0, 0),
        75,
        TechRequirement::Technology(Tech::GuardTower),
        TechEffect::BuildingStats,
    ),
    // DAT tech 46
    generic(
        Tech::Devotion,
        BuildingKind::Monastery,
        Age::Castle,
        cost(100, 0, 200, 0),
        40,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 45
    generic(
        Tech::Faith,
        BuildingKind::Monastery,
        Age::Imperial,
        cost(550, 0, 750, 0),
        60,
        TechRequirement::Technology(Tech::Devotion),
        TechEffect::Passive,
    ),
    // DAT tech 252
    generic(
        Tech::Fervor,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 140, 0),
        50,
        TechRequirement::None,
        TechEffect::UnitSpeed {
            target: TechTarget::Monk,
            percent: 115,
        },
    ),
    // DAT tech 231
    generic(
        Tech::Sanctity,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 175, 0),
        60,
        TechRequirement::None,
        TechEffect::UnitHp {
            target: TechTarget::Monk,
            hp: 15,
        },
    ),
    // DAT tech 441
    generic(
        Tech::HerbalMedicine,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 200, 0),
        35,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 438
    generic(
        Tech::Theocracy,
        BuildingKind::Monastery,
        Age::Imperial,
        cost(0, 0, 200, 0),
        75,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 233
    generic(
        Tech::Illumination,
        BuildingKind::Monastery,
        Age::Imperial,
        cost(0, 0, 120, 0),
        65,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 230
    generic(
        Tech::BlockPrinting,
        BuildingKind::Monastery,
        Age::Imperial,
        cost(0, 0, 200, 0),
        55,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 319
    generic(
        Tech::Atonement,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 325, 0),
        40,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 316
    generic(
        Tech::Redemption,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 475, 0),
        50,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 439
    generic(
        Tech::Heresy,
        BuildingKind::Monastery,
        Age::Castle,
        cost(0, 0, 1000, 0),
        60,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 315
    generic(
        Tech::Conscription,
        BuildingKind::Castle,
        Age::Imperial,
        cost(150, 0, 150, 0),
        60,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 379
    generic(
        Tech::Hoardings,
        BuildingKind::Castle,
        Age::Imperial,
        cost(400, 400, 0, 0),
        75,
        TechRequirement::None,
        TechEffect::BuildingStats,
    ),
    // DAT tech 321
    generic(
        Tech::Sappers,
        BuildingKind::Castle,
        Age::Imperial,
        cost(400, 200, 0, 0),
        10,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 408
    generic(
        Tech::Spies,
        BuildingKind::Castle,
        Age::Imperial,
        cost(0, 0, 200, 0),
        1,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 48
    generic(
        Tech::Caravan,
        BuildingKind::Market,
        Age::Castle,
        cost(200, 0, 200, 0),
        40,
        TechRequirement::None,
        TechEffect::UnitSpeed {
            target: TechTarget::Trade,
            percent: 120,
        },
    ),
    // DAT tech 15
    generic(
        Tech::Guilds,
        BuildingKind::Market,
        Age::Imperial,
        cost(300, 0, 200, 0),
        50,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 17
    generic(
        Tech::Banking,
        BuildingKind::Market,
        Age::Imperial,
        cost(300, 0, 200, 0),
        70,
        TechRequirement::Technology(Tech::Coinage),
        TechEffect::Passive,
    ),
    // DAT tech 23
    generic(
        Tech::Coinage,
        BuildingKind::Market,
        Age::Castle,
        cost(200, 0, 100, 0),
        70,
        TechRequirement::None,
        TechEffect::Passive,
    ),
    // DAT tech 54
    generic(
        Tech::TreadmillCrane,
        BuildingKind::University,
        Age::Castle,
        cost(0, 200, 0, 50),
        20,
        TechRequirement::None,
        TechEffect::Passive,
    ),
];
