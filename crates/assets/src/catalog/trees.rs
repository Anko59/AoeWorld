//! Trial DAT tree stages. Species `_SN` candidates are absent from graphics.drs;
//! every live tree instead references the available TREEA_SN dying graphic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeStages {
    pub unit: i32,
    pub full_slp: u32,
    pub chopped_graphic: i32,
    pub chopped_slp: u32,
    pub stump_unit: i32,
    pub stump_slp: u32,
}

pub const STAGES: [TreeStages; 3] = [tree(349, 4652), tree(350, 4654), tree(351, 4653)];

const fn tree(unit: i32, full_slp: u32) -> TreeStages {
    TreeStages {
        unit,
        full_slp,
        chopped_graphic: 1699,
        chopped_slp: 1252,
        stump_unit: 415,
        stump_slp: 1035,
    }
}
