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
    FishingShipStand,
    FishingShipWalk,
    FishingShipWork,
    FisherWork,
    ArcherStand,
    ArcherWalk,
    ArcherAttack,
    ArcherDie,
    ArcherDecay,
    SkirmisherStand,
    SkirmisherWalk,
    SkirmisherAttack,
    SkirmisherDie,
    SkirmisherDecay,
    SpearmanStand,
    SpearmanWalk,
    SpearmanAttack,
    SpearmanDie,
    SpearmanDecay,
    ManAtArmsStand,
    ManAtArmsWalk,
    ManAtArmsAttack,
    ManAtArmsDie,
    ManAtArmsDecay,
    ScoutStand,
    ScoutWalk,
    Arrow,
    Javelin,
    /// DAT unit 259 (VMFAR) task: tilling a farm.
    FarmerWork,
}

#[derive(Clone, Copy)]
pub struct ActorSource {
    pub id: u32,
    pub stride: u8,
    /// Rounded milliseconds per native cell, not one universal animation rate.
    pub millis: u16,
}
impl ActorSource {
    /// Bound the fisherman atlas rows while retaining their native loop duration.
    pub const fn sample_step(self) -> u8 {
        if self.stride >= 8 && self.id != 990 {
            2
        } else {
            1
        }
    }
    pub const fn frames(self) -> u32 {
        match self.id {
            3799 => 176,
            3820 => 170,
            _ => self.stride as u32 * 5,
        }
    }
    pub const fn selected_frames(self) -> usize {
        self.stride.div_ceil(self.sample_step()) as usize * 5
    }
    pub const fn source_frame(self, row: usize, phase: usize) -> usize {
        match self.id {
            3799 => [0, 44, 88, 132, 165][row],
            3820 => row * 40,
            _ => row * self.stride as usize + phase,
        }
    }
    pub fn manifest_source(self) -> String {
        format!("graphics.drs:[32, 112, 108, 115]:{}", self.id)
    }
}
const fn sprite(id: u32, stride: u8, millis: u16) -> ActorSource {
    ActorSource { id, stride, millis }
}

pub const SPRITES: [ActorSource; 75] = [
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
    sprite(444, 1, 200),
    sprite(449, 1, 200),
    sprite(444, 1, 200),
    sprite(3575, 28, 200),
    sprite(8, 10, 200),
    sprite(12, 10, 70),
    sprite(2, 10, 70),
    sprite(5, 10, 100),
    sprite(9, 5, 6000),
    sprite(1650, 8, 1000),
    sprite(1654, 10, 70),
    sprite(1644, 12, 100),
    sprite(1647, 10, 100),
    sprite(1651, 5, 6000),
    sprite(873, 9, 120),
    sprite(877, 10, 70),
    sprite(867, 10, 100),
    sprite(870, 10, 100),
    sprite(874, 5, 6000),
    sprite(1044, 11, 250),
    sprite(1048, 11, 85),
    sprite(1038, 11, 160),
    sprite(1041, 11, 100),
    sprite(1045, 5, 6000),
    sprite(2085, 10, 200),
    sprite(2089, 10, 110),
    sprite(3799, 1, 100),
    sprite(3820, 1, 100),
    sprite(1512, 15, 133),
];

#[cfg(test)]
#[path = "tests/units.rs"]
mod tests;
