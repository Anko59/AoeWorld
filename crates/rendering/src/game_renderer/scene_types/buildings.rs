use super::SceneUnitKind;
use SceneUnitKind::*;

#[derive(Clone, Copy)]
struct Meta {
    id: i16,
    slot: u8,
    side: u8,
    flags: u8,
}
const EMPTY: Meta = Meta {
    id: -1,
    slot: u8::MAX,
    side: 0,
    flags: 0,
};
const FOUNDATION: u8 = 1;
const RUBBLE: u8 = 2;

// Stable semantic slots; actor tags never enter the building metadata table.
const ROWS: [(SceneUnitKind, i16, u8); 88] = [
    (BuildingTownCenter, 109, 4),
    (House, 70, 2),
    (LumberCamp, 562, 2),
    (MiningCamp, 584, 2),
    (Mill, 68, 2),
    (Farm, 50, 3),
    (Barracks, 12, 3),
    (PalisadeWall, 72, 1),
    (Outpost, 598, 1),
    (ConstructionPalisadeWall, 72, 1),
    (ConstructionHouse, 70, 2),
    (ConstructionBarracks, 12, 3),
    (ConstructionTownCenter, 109, 4),
    (RubbleTownCenter, 109, 4),
    (RubbleHouse, 70, 2),
    (RubbleLumberCamp, 562, 2),
    (RubbleMiningCamp, 584, 2),
    (RubbleMill, 68, 2),
    (RubbleFarm, 50, 3),
    (RubbleBarracks, 12, 3),
    (RubblePalisadeWall, 72, 1),
    (RubbleOutpost, 598, 1),
    (SceneUnitKind::BuilderWork, -1, 1),
    (Dock, 45, 3),
    (FoundationDock, 45, 3),
    (RubbleDock, 45, 3),
    (ArcheryRange, 87, 3),
    (Stable, 101, 3),
    (Blacksmith, 103, 3),
    (Market, 84, 4),
    (WatchTower, 79, 1),
    (PalisadeGate, 796, 4),
    (RubbleArcheryRange, 87, 3),
    (RubbleStable, 101, 3),
    (RubbleBlacksmith, 103, 3),
    (RubbleMarket, 84, 4),
    (RubbleWatchTower, 79, 1),
    (RubblePalisadeGate, 796, 4),
    (FeudalTownCenter, 71, 4),
    (FeudalHouse, 463, 2),
    (FeudalMill, 129, 2),
    (FeudalBarracks, 498, 3),
    (OpenPalisadeGate, 796, 4),
    (GateFoundation, 796, 4),
    (FeudalDock, 133, 3),
    (Castle, 82, 4),
    (Monastery, 104, 3),
    (University, 209, 4),
    (SiegeWorkshop, 49, 4),
    (RubbleCastle, 82, 4),
    (RubbleMonastery, 104, 3),
    (RubbleUniversity, 209, 4),
    (RubbleSiegeWorkshop, 49, 4),
    (CastleTownCenter, 141, 4),
    (CastleHouse, 464, 2),
    (CastleLumberCamp, 564, 2),
    (CastleMiningCamp, 586, 2),
    (CastleMill, 130, 2),
    (CastleBarracks, 132, 3),
    (CastleDock, 47, 3),
    (CastleArcheryRange, 10, 3),
    (CastleStable, 86, 3),
    (CastleBlacksmith, 18, 3),
    (CastleMarket, 116, 4),
    (CastleMonastery, 31, 3),
    (BridgeATop, 605, 1),
    (BridgeAMiddle, 606, 1),
    (BridgeABottom, 607, 1),
    (BridgeBTop, 608, 1),
    (BridgeBMiddle, 609, 1),
    (BridgeBBottom, 610, 1),
    (StoneWall, 117, 1),
    (StoneGate, 490, 4),
    (OpenStoneGate, 490, 4),
    (ImperialTownCenter, 142, 4),
    (ImperialHouse, 465, 2),
    (ImperialLumberCamp, 565, 2),
    (ImperialMiningCamp, 587, 2),
    (ImperialMill, 131, 2),
    (ImperialBarracks, 20, 3),
    (ImperialDock, 51, 3),
    (ImperialArcheryRange, 14, 3),
    (ImperialStable, 153, 3),
    (ImperialBlacksmith, 19, 3),
    (ImperialMarket, 137, 4),
    (ImperialMonastery, 32, 3),
    (ImperialUniversity, 210, 4),
    (ImperialSiegeWorkshop, 150, 4),
];

// Construction retains the selected DAT age identity.
const FOUNDATIONS: [(SceneUnitKind, i16, u8, u8); 53] = [
    (ConstructionTownCenter, 109, 4, 12),
    (ConstructionHouse, 70, 2, 10),
    (ConstructionLumberCamp, 562, 2, 10),
    (ConstructionMiningCamp, 584, 2, 10),
    (ConstructionMill, 68, 2, 10),
    (ConstructionFarm, 50, 3, 5),
    (ConstructionBarracks, 12, 3, 11),
    (ConstructionPalisadeWall, 72, 1, 9),
    (ConstructionOutpost, 598, 1, 9),
    (ConstructionRange, 87, 3, 11),
    (ConstructionStable, 101, 3, 11),
    (ConstructionBlacksmith, 103, 3, 11),
    (ConstructionMarket, 84, 4, 12),
    (ConstructionTower, 79, 1, 9),
    (ConstructionGate, 796, 4, 43),
    (ConstructionFeudalTownCenter, 71, 4, 12),
    (ConstructionFeudalHouse, 463, 2, 10),
    (ConstructionFeudalMill, 129, 2, 10),
    (ConstructionFeudalLumberCamp, 563, 2, 10),
    (ConstructionFeudalMiningCamp, 585, 2, 10),
    (ConstructionFeudalBarracks, 498, 3, 11),
    (ConstructionFeudalDock, 133, 3, 24),
    (ConstructionCastle, 82, 4, 12),
    (ConstructionMonastery, 104, 3, 11),
    (ConstructionUniversity, 209, 4, 12),
    (ConstructionSiegeWorkshop, 49, 4, 12),
    (FoundationDock, 45, 3, 24),
    (ConstructionCastleTownCenter, 141, 4, 12),
    (ConstructionCastleHouse, 464, 2, 10),
    (ConstructionCastleLumberCamp, 564, 2, 10),
    (ConstructionCastleMiningCamp, 586, 2, 10),
    (ConstructionCastleMill, 130, 2, 10),
    (ConstructionCastleBarracks, 132, 3, 11),
    (ConstructionCastleDock, 47, 3, 24),
    (ConstructionCastleArcheryRange, 10, 3, 11),
    (ConstructionCastleStable, 86, 3, 11),
    (ConstructionCastleBlacksmith, 18, 3, 11),
    (ConstructionCastleMarket, 116, 4, 12),
    (ConstructionCastleMonastery, 31, 3, 11),
    (ConstructionImperialTownCenter, 142, 4, 12),
    (ConstructionImperialHouse, 465, 2, 10),
    (ConstructionImperialLumberCamp, 565, 2, 10),
    (ConstructionImperialMiningCamp, 587, 2, 10),
    (ConstructionImperialMill, 131, 2, 10),
    (ConstructionImperialBarracks, 20, 3, 11),
    (ConstructionImperialDock, 51, 3, 24),
    (ConstructionImperialArcheryRange, 14, 3, 11),
    (ConstructionImperialStable, 153, 3, 11),
    (ConstructionImperialBlacksmith, 19, 3, 11),
    (ConstructionImperialMarket, 137, 4, 12),
    (ConstructionImperialMonastery, 32, 3, 11),
    (ConstructionImperialUniversity, 210, 4, 12),
    (ConstructionImperialSiegeWorkshop, 150, 4, 12),
];
const META: [Meta; SceneUnitKind::Actor0 as usize] = {
    let mut out = [EMPTY; SceneUnitKind::Actor0 as usize];
    let mut i = 0;
    while i < ROWS.len() {
        let (kind, id, side) = ROWS[i];
        let tag = kind as usize;
        if tag < out.len() {
            let rubble =
                (i >= 13 && i <= 21) || i == 25 || (i >= 32 && i <= 37) || (i >= 49 && i <= 52);
            out[tag] = Meta {
                id,
                slot: i as u8,
                side,
                flags: if i == 43 {
                    FOUNDATION
                } else if rubble {
                    RUBBLE
                } else {
                    0
                },
            };
        }
        i += 1;
    }
    i = 0;
    while i < FOUNDATIONS.len() {
        let (kind, id, side, slot) = FOUNDATIONS[i];
        out[kind as usize] = Meta {
            id,
            slot,
            side,
            flags: FOUNDATION,
        };
        i += 1;
    }
    out[TownCenter as usize] = Meta {
        id: 109,
        slot: u8::MAX,
        side: 4,
        flags: 0,
    };
    out[FeudalLumberCamp as usize] = Meta {
        id: 563,
        slot: 2,
        side: 2,
        flags: 0,
    };
    out[FeudalMiningCamp as usize] = Meta {
        id: 585,
        slot: 3,
        side: 2,
        flags: 0,
    };
    out
};
impl SceneUnitKind {
    fn building_meta(self) -> Meta {
        META.get(self as usize).copied().unwrap_or(EMPTY)
    }
    pub fn bridge(along_x: bool, piece: u8) -> Self {
        Self::building((if along_x { 68 } else { 65 }) + piece.min(2))
    }
    pub fn building(slot: u8) -> Self {
        ROWS[usize::from(slot).min(ROWS.len() - 1)].0
    }
    pub(in crate::game_renderer) fn building_slot(self) -> Option<u8> {
        let slot = self.building_meta().slot;
        (slot != u8::MAX).then_some(slot)
    }
    pub fn foundation(id: i16) -> Self {
        FOUNDATIONS
            .iter()
            .find(|row| row.1 == id)
            .map_or(ConstructionOutpost, |row| row.0)
    }
    pub(in crate::game_renderer) fn is_foundation(self) -> bool {
        self.building_meta().flags & FOUNDATION != 0
    }
    pub(in crate::game_renderer) fn rubble(self) -> bool {
        self.building_meta().flags & RUBBLE != 0
    }
    pub fn building_id(self) -> Option<i32> {
        let id = self.building_meta().id;
        (id >= 0).then_some(i32::from(id))
    }
    /// Gate axes select complete DAT assemblies, including their corner towers.
    /// Bit 4 carries the axis independently of the low construction-stage bits.
    pub fn oriented_building_id(self, facing: u8) -> Option<i32> {
        match self.building_id()? {
            490 if facing & 16 != 0 => Some(487),
            796 if facing & 16 != 0 => Some(792),
            id => Some(id),
        }
    }
    pub(in crate::game_renderer) fn building_side(self) -> Option<f64> {
        let side = self.building_meta().side;
        (side != 0).then_some(f64::from(side))
    }
}
