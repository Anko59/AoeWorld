use super::SceneUnitKind;
impl SceneUnitKind {
    pub fn bridge(along_x: bool, piece: u8) -> Self {
        Self::building((if along_x { 68 } else { 65 }) + piece.min(2))
    }

    /// Map building roles to the atlas's semantic order.
    pub fn building(slot: u8) -> Self {
        use SceneUnitKind::*;
        [
            BuildingTownCenter,
            House,
            LumberCamp,
            MiningCamp,
            Mill,
            Farm,
            Barracks,
            PalisadeWall,
            Outpost,
            Foundation1x1,
            Foundation2x2,
            Foundation3x3,
            Foundation4x4,
            RubbleTownCenter,
            RubbleHouse,
            RubbleLumberCamp,
            RubbleMiningCamp,
            RubbleMill,
            RubbleFarm,
            RubbleBarracks,
            RubblePalisadeWall,
            RubbleOutpost,
            BuilderAtlas,
            Dock,
            FoundationDock,
            RubbleDock,
            ArcheryRange,
            Stable,
            Blacksmith,
            Market,
            WatchTower,
            PalisadeGate,
            RubbleArcheryRange,
            RubbleStable,
            RubbleBlacksmith,
            RubbleMarket,
            RubbleWatchTower,
            RubblePalisadeGate,
            FeudalTownCenter,
            FeudalHouse,
            FeudalMill,
            FeudalBarracks,
            OpenPalisadeGate,
            GateFoundation,
            FeudalDock,
            Castle,
            Monastery,
            University,
            SiegeWorkshop,
            RubbleCastle,
            RubbleMonastery,
            RubbleUniversity,
            RubbleSiegeWorkshop,
            CastleTownCenter,
            CastleHouse,
            CastleLumberCamp,
            CastleMiningCamp,
            CastleMill,
            CastleBarracks,
            CastleDock,
            CastleArcheryRange,
            CastleStable,
            CastleBlacksmith,
            CastleMarket,
            CastleMonastery,
            BridgeATop,
            BridgeAMiddle,
            BridgeABottom,
            BridgeBTop,
            BridgeBMiddle,
            BridgeBBottom,
            StoneWall,
            RubbleStoneWall,
            StoneGate,
            OpenStoneGate,
            RubbleStoneGate,
            StoneGateFoundation,
        ][usize::from(slot.min(76))]
    }
    pub(in crate::game_renderer) fn building_slot(self) -> Option<u8> {
        match self {
            Self::BridgeATop => return Some(65),
            Self::BridgeAMiddle => return Some(66),
            Self::BridgeABottom => return Some(67),
            Self::BridgeBTop => return Some(68),
            Self::BridgeBMiddle => return Some(69),
            Self::BridgeBBottom => return Some(70),
            Self::StoneWall => return Some(71),
            Self::RubbleStoneWall => return Some(72),
            Self::StoneGate => return Some(73),
            Self::OpenStoneGate => return Some(74),
            Self::RubbleStoneGate => return Some(75),
            Self::StoneGateFoundation => return Some(76),
            Self::CastleTownCenter => return Some(53),
            Self::CastleHouse => return Some(54),
            Self::CastleLumberCamp => return Some(55),
            Self::CastleMiningCamp => return Some(56),
            Self::CastleMill => return Some(57),
            Self::CastleBarracks => return Some(58),
            Self::CastleDock => return Some(59),
            Self::CastleArcheryRange => return Some(60),
            Self::CastleStable => return Some(61),
            Self::CastleBlacksmith => return Some(62),
            Self::CastleMarket => return Some(63),
            Self::CastleMonastery => return Some(64),

            Self::Castle => return Some(45),
            Self::Monastery => return Some(46),
            Self::University => return Some(47),
            Self::SiegeWorkshop => return Some(48),
            Self::RubbleCastle => return Some(49),
            Self::RubbleMonastery => return Some(50),
            Self::RubbleUniversity => return Some(51),
            Self::RubbleSiegeWorkshop => return Some(52),
            Self::Dock => return Some(23),
            Self::FoundationDock => return Some(24),
            Self::RubbleDock => return Some(25),
            Self::BuilderAtlas => return Some(22),
            Self::FeudalLumberCamp => return Some(2),
            Self::FeudalMiningCamp => return Some(3),
            _ => {}
        }
        if self.is_foundation() {
            return Some(match self.building_id()? {
                // The DAT farm has no construction graphic: a farm being
                // built is its own foundation terrain, never scaffolding.
                50 => 5,
                109 | 71 | 84 | 82 | 209 | 49 => 12,
                104 => 11,
                12 | 87 | 101 | 103 | 498 => 11,
                72 | 598 | 79 => 9,
                792 => 43,
                133 => 24,
                _ => 10,
            });
        }
        (self as u8)
            .checked_sub(Self::BuildingTownCenter as u8)
            .filter(|slot| *slot <= 21)
            .or_else(|| {
                (self as u8)
                    .checked_sub(Self::ArcheryRange as u8)
                    .filter(|slot| *slot <= 18)
                    .map(|slot| slot + 26)
            })
    }

    pub fn foundation(id: i16) -> Self {
        use SceneUnitKind::*;
        match id {
            141 => ConstructionFeudalTownCenter,
            464 => ConstructionFeudalHouse,
            564 => ConstructionFeudalLumberCamp,
            586 => ConstructionFeudalMiningCamp,
            130 => ConstructionFeudalMill,
            132 => ConstructionFeudalBarracks,
            47 => ConstructionFeudalDock,
            10 => ConstructionRange,
            86 => ConstructionStable,
            18 => ConstructionBlacksmith,
            116 => ConstructionMarket,
            31 => ConstructionMonastery,

            45 => FoundationDock,
            82 => ConstructionCastle,
            104 => ConstructionMonastery,
            209 => ConstructionUniversity,
            49 => ConstructionSiegeWorkshop,
            109 => ConstructionTownCenter,
            70 => ConstructionHouse,
            562 => ConstructionLumberCamp,
            584 => ConstructionMiningCamp,
            68 => ConstructionMill,
            50 => ConstructionFarm,
            12 => ConstructionBarracks,
            72 => ConstructionPalisadeWall,
            87 => ConstructionRange,
            101 => ConstructionStable,
            103 => ConstructionBlacksmith,
            84 => ConstructionMarket,
            79 => ConstructionTower,
            792 => ConstructionGate,
            71 => ConstructionFeudalTownCenter,
            463 => ConstructionFeudalHouse,
            129 => ConstructionFeudalMill,
            563 => ConstructionFeudalLumberCamp,
            585 => ConstructionFeudalMiningCamp,
            498 => ConstructionFeudalBarracks,
            133 => ConstructionFeudalDock,
            _ => ConstructionOutpost,
        }
    }
    pub(in crate::game_renderer) fn is_foundation(self) -> bool {
        matches!(
            self,
            Self::ConstructionCastle
                | Self::ConstructionMonastery
                | Self::ConstructionUniversity
                | Self::ConstructionSiegeWorkshop
        ) || self == Self::FoundationDock
            || (Self::ConstructionTownCenter as u8..=Self::ConstructionFeudalDock as u8)
                .contains(&(self as u8))
    }
    pub fn building_id(self) -> Option<i32> {
        use SceneUnitKind::*;
        match self {
            Self::ConstructionCastle => return Some(82),
            Self::ConstructionMonastery => return Some(104),
            Self::ConstructionUniversity => return Some(209),
            Self::ConstructionSiegeWorkshop => return Some(49),
            _ => {}
        }
        if self.is_foundation() && self != FoundationDock {
            return [
                109, 70, 562, 584, 68, 50, 12, 72, 598, 87, 101, 103, 84, 79, 792, 71, 463, 129,
                563, 585, 498, 133,
            ]
            .get((self as u8 - Self::ConstructionTownCenter as u8) as usize)
            .copied();
        }
        Some(match self {
            CastleTownCenter => 141,
            CastleHouse => 464,
            CastleLumberCamp => 564,
            CastleMiningCamp => 586,
            CastleMill => 130,
            CastleBarracks => 132,
            CastleDock => 47,
            CastleArcheryRange => 10,
            CastleStable => 86,
            CastleBlacksmith => 18,
            CastleMarket => 116,
            CastleMonastery => 31,

            TownCenter | BuildingTownCenter => 109,
            House => 70,
            LumberCamp => 562,
            MiningCamp => 584,
            Mill => 68,
            Farm => 50,
            Barracks => 12,
            PalisadeWall => 72,
            Outpost => 598,
            Dock | FoundationDock => 45,
            ArcheryRange => 87,
            Stable => 101,
            Blacksmith => 103,
            Market => 84,
            WatchTower => 79,
            PalisadeGate | OpenPalisadeGate | GateFoundation => 792,
            FeudalTownCenter => 71,
            FeudalHouse => 463,
            FeudalMill => 129,
            FeudalLumberCamp => 563,
            FeudalMiningCamp => 585,
            FeudalBarracks => 498,
            FeudalDock => 133,
            Castle => 82,
            Monastery => 104,
            University => 209,
            SiegeWorkshop => 49,
            _ => return None,
        })
    }
    pub(in crate::game_renderer) fn building_side(self) -> Option<f64> {
        if self == Self::TownCenter {
            return Some(4.0);
        }
        let slot = self.building_slot()?;
        Some(match slot {
            0
            | 12
            | 13
            | 29
            | 31
            | 35
            | 37
            | 38
            | 42
            | 43
            | 45
            | 47
            | 48
            | 49
            | 51
            | 52
            | 53
            | 63
            | 73..=76 => 4.0,
            1..=4 | 10 | 14..=17 | 39 | 40 | 54..=57 => 2.0,
            5 | 6 | 11 | 18 | 19 | 23..=28 | 32..=34 | 41 | 44 | 46 | 50 | 58..=62 | 64 => 3.0,
            _ => 1.0,
        })
    }
}
