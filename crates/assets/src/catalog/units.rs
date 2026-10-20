//! Direction rows and frame durations decoded from the owner's VER 5.7 dat.
//! Five stored rows supply eight facings. Pixels remain in the private pack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Pose {
    MilitiaStand,
    MilitiaWalk,
    MilitiaAttack,
    MilitiaDie,
    VillagerAttack,
    VillagerDie,
    HunterStand,
    HunterWalk,
    HunterAttack,
    Butcher,
    HunterCarry,
    ShepherdStand,
    ShepherdWalk,
    ShepherdWork,
    SheepStand,
    SheepWalk,
    SheepDie,
    DeerStand,
    DeerWalk,
    DeerDie,
    BoarStand,
    BoarWalk,
    BoarAttack,
    BoarDie,
    WolfStand,
    WolfWalk,
    WolfRun,
    WolfAttack,
    WolfDie,
    ScoutAttack,
    ScoutDie,
    MilitiaDecay,
    VillagerDecay,
    SheepDecay,
    DeerDecay,
    BoarDecay,
    WolfDecay,
    ScoutDecay,
    HunterDie,
    HunterDecay,
    ShepherdDie,
    ShepherdDecay,
    NeutralSheepStand,
    NeutralSheepWalk,
    NeutralSheepDie,
    NeutralSheepDecay,
}

#[derive(Clone, Copy)]
pub struct ActorSource {
    pub id: u32,
    pub stride: u8,
    /// Rounded milliseconds per native cell, not one universal animation rate.
    pub millis: u16,
}
impl ActorSource {
    pub const fn frames(self) -> u32 {
        self.stride as u32 * 5
    }
    pub fn manifest_source(self) -> String {
        format!("graphics.drs:[32, 112, 108, 115]:{}", self.id)
    }
}
const fn sprite(id: u32, stride: u8, millis: u16) -> ActorSource {
    ActorSource { id, stride, millis }
}

pub const SPRITES: [ActorSource; 46] = [
    sprite(993, 6, 1000),
    sprite(997, 12, 70),
    sprite(987, 10, 100),
    sprite(990, 10, 100),
    sprite(1473, 15, 70),
    sprite(1476, 15, 100),
    sprite(1525, 15, 200),
    sprite(1531, 15, 70),
    sprite(1518, 15, 100),
    sprite(1528, 15, 100),
    sprite(1519, 15, 70),
    sprite(3684, 15, 200),
    sprite(3686, 15, 70),
    sprite(3682, 15, 100),
    sprite(3629, 9, 275),
    sprite(3634, 16, 50),
    sprite(3626, 13, 100),
    sprite(342, 5, 275),
    sprite(348, 15, 50),
    sprite(339, 14, 100),
    sprite(2557, 10, 120),
    sprite(2559, 10, 100),
    sprite(2555, 17, 95),
    sprite(2556, 11, 100),
    sprite(1635, 10, 100),
    sprite(1640, 10, 90),
    sprite(1636, 12, 75),
    sprite(1629, 20, 85),
    sprite(1632, 10, 100),
    sprite(2079, 10, 135),
    sprite(2082, 10, 190),
    sprite(994, 5, 6000),
    sprite(1481, 5, 6000),
    sprite(3631, 5, 6000),
    sprite(344, 5, 6000),
    sprite(2558, 5, 6000),
    sprite(1637, 5, 6000),
    sprite(2086, 5, 6000),
    sprite(1522, 15, 100),
    sprite(1527, 5, 6000),
    sprite(3683, 15, 100),
    sprite(4649, 5, 6000),
    sprite(3629, 9, 275),
    sprite(3634, 16, 50),
    sprite(3626, 13, 100),
    sprite(3631, 5, 6000),
];

#[cfg(test)]
#[path = "tests/units.rs"]
mod tests;
