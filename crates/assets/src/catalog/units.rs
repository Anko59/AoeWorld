//! Native actor layouts and durations; pixels remain in the private pack.
//! Every unit uses DAT-native directions and timelines.
#[path = "units/native.rs"]
mod native;
#[path = "units/projectile.rs"]
mod projectile;
pub use projectile::{PROJECTILE_PHASES, projectile_facing, release_frame};
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
    LongSwordsmanStand,
    LongSwordsmanWalk,
    LongSwordsmanAttack,
    LongSwordsmanDie,
    LongSwordsmanDecay,
    PikemanStand,
    PikemanWalk,
    PikemanAttack,
    PikemanDie,
    PikemanDecay,
    CrossbowmanStand,
    CrossbowmanWalk,
    CrossbowmanAttack,
    CrossbowmanDie,
    CrossbowmanDecay,
    EliteSkirmisherStand,
    EliteSkirmisherWalk,
    EliteSkirmisherAttack,
    EliteSkirmisherDie,
    EliteSkirmisherDecay,
    KnightStand,
    KnightWalk,
    KnightAttack,
    KnightDie,
    KnightDecay,
    LightCavalryStand,
    LightCavalryWalk,
    LightCavalryAttack,
    LightCavalryDie,
    LightCavalryDecay,
    MonkStand,
    MonkWalk,
    MonkAttack,
    MonkDie,
    MonkDecay,
    MangonelStand,
    MangonelWalk,
    MangonelAttack,
    MangonelDie,
    MangonelDecay,
    BatteringRamStand,
    BatteringRamWalk,
    BatteringRamAttack,
    BatteringRamDie,
    BatteringRamDecay,
    Stone,
    Galley,
    WarGalley,
    FireShip,
    DemolitionShip,
    TransportShip,
    KingStand,
    KingWalk,
    KingDie,
    KingDecay,
    GeneralStand,
    GeneralWalk,
    GeneralAttack,
    GeneralDie,
    GeneralDecay,
    VillagerStand,
    VillagerWalk,
    ChopperWork,
    CarryWood,
    ForagerWork,
    CarryFood,
    CarryGold,
    CarryStone,
    MinerWork,
    BuilderWork,
    FishingShipDie,
    FishingShipDecay,
    GalleyWalk,
    GalleyAttack,
    GalleyDie,
    GalleyDecay,
    WarGalleyWalk,
    WarGalleyAttack,
    WarGalleyDie,
    WarGalleyDecay,
    FireShipWalk,
    FireShipAttack,
    FireShipDie,
    FireShipDecay,
    DemolitionShipWalk,
    DemolitionShipAttack,
    DemolitionShipDie,
    DemolitionShipDecay,
    TransportShipWalk,
    TransportShipAttack,
    TransportShipDie,
    TransportShipDecay,
}

#[derive(Clone, Copy)]
pub struct ActorSource {
    pub id: u32,
    pub stride: u8,
    /// Rounded milliseconds per native cell, not one universal animation rate.
    pub millis: u16,
    /// Upper bound on selected native cells per facing; zero uses the base policy.
    pub phases: u8,
}
impl ActorSource {
    pub const fn is_native(self) -> bool {
        native::get(self.id).is_some() || self.id == 4500
    }
    /// Native DE durations retain fractional milliseconds (DAT f32 seconds).
    pub fn frame_millis(self) -> f64 {
        if let Some(graphic) = native::get(self.id) {
            return if self.stride == 1 && graphic.stride > 1 {
                0.0
            } else {
                f64::from(graphic.seconds) * 1000.0
            };
        }
        if self.id == 4500 {
            return f64::from(1.0_f32 / 60.0) * 1000.0;
        }
        f64::from(self.millis)
    }
    pub const fn replay_delay_seconds(self) -> f32 {
        match native::get(self.id) {
            Some(g) => g.replay_delay,
            None => 0.0,
        }
    }
    pub const fn sample_step(self) -> u8 {
        if native::get(self.id).is_some() {
            return 1;
        }
        if self.phases > 0 {
            return self.stride.div_ceil(self.phases);
        }
        if self.stride >= 8 && self.id != 990 {
            2
        } else {
            1
        }
    }
    pub const fn frames(self) -> u32 {
        if let Some(g) = native::get(self.id) {
            return g.cells as u32;
        }
        match self.id {
            4500 => 30,
            3799 => 176,
            3820 => 170,
            _ => self.stride as u32 * 5,
        }
    }
    pub const fn rows(self) -> usize {
        if let Some(g) = native::get(self.id) {
            g.angles as usize
        } else if self.id == 4500 {
            1
        } else {
            5
        }
    }
    pub const fn selected_frames(self) -> usize {
        self.stride.div_ceil(self.sample_step()) as usize * self.rows()
    }
    pub const fn source_frame(self, row: usize, phase: usize) -> usize {
        if let Some(g) = native::get(self.id) {
            return row * g.stride as usize
                + if self.stride == 1 && g.stride > 1 {
                    g.stride as usize - 1
                } else {
                    phase
                };
        }
        match self.id {
            4500 => phase,
            3799 | 3820 => projectile::source_cell(self.id, row),
            _ => row * self.stride as usize + phase,
        }
    }
    pub fn manifest_source(self) -> String {
        if let Some(g) = native::get(self.id) {
            return format!("de/{}.sld", g.stem);
        }
        if self.id == 4500 {
            return "de/p_mangonel_x1.sld".to_owned();
        }
        format!("graphics.drs:[32, 112, 108, 115]:{}", self.id)
    }
}
const fn sprite(id: u32, stride: u8, millis: u16) -> ActorSource {
    ActorSource {
        id,
        stride,
        millis,
        phases: 0,
    }
}

#[path = "units/generated.rs"]
mod generated;
pub use generated::SPRITES;

#[cfg(test)]
#[path = "tests/units.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/native_units.rs"]
mod native_tests;
