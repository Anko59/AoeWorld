use super::{Age, BuildingKind};

impl BuildingKind {
    /// DE's upgraded unit records, plus the automatic palisade HP effect 293.
    pub fn age_stats(self, age: Age) -> (u16, u8) {
        if age == Age::Castle {
            use BuildingKind::*;
            return match self {
                TownCenter => (2400, 8),
                WatchTower => (1020, 10),
                Outpost => (500, 12),
                House => (900, 2),
                LumberCamp | MiningCamp | Mill => (1000, 6),
                Barracks | ArcheryRange | Stable => (1800, 6),
                Blacksmith | Market | Dock => (2100, self.rules().los),
                PalisadeGate => (400, 6),
                _ => (self.rules().max_hp, self.rules().los),
            };
        }
        if age >= Age::Feudal {
            use BuildingKind::*;
            match self {
                House => (750, 2),
                LumberCamp | MiningCamp | Mill => (800, 6),
                Barracks => (1500, 6),
                PalisadeGate => (400, 6),
                _ => (self.rules().max_hp, self.rules().los),
            }
        } else {
            (self.rules().max_hp, self.rules().los)
        }
    }
}
