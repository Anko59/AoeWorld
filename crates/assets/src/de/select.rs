//! The sources imported from a Definitive Edition install.

/// One `<stem>_x1.sld` graphic and what it depicts.
#[derive(Debug)]
pub struct Item {
    pub building: &'static str,
    pub role: &'static str,
    pub stem: &'static str,
}

/// Farm ground textures under `terrain/textures/2x` (no farm SLD exists).
pub const FARM_TEXTURES: &[&str] = &["g_fm1", "g_fm2"];

pub const SELECTION: &[Item] = &[
    Item {
        building: "house",
        role: "standing",
        stem: "b_dark_house_age1",
    },
    Item {
        building: "house",
        role: "destruction",
        stem: "b_dark_house_age1_destruction",
    },
    Item {
        building: "house",
        role: "rubble",
        stem: "b_dark_house_age1_rubble",
    },
    Item {
        building: "mill",
        role: "standing",
        stem: "b_dark_mill_age1",
    },
    Item {
        building: "mill",
        role: "destruction",
        stem: "b_dark_mill_age1_destruction",
    },
    Item {
        building: "mill",
        role: "rubble",
        stem: "b_dark_mill_age1_rubble",
    },
    Item {
        building: "barracks",
        role: "standing",
        stem: "b_dark_barracks_age1",
    },
    Item {
        building: "barracks",
        role: "destruction",
        stem: "b_dark_barracks_age1_destruction",
    },
    Item {
        building: "barracks",
        role: "rubble",
        stem: "b_dark_barracks_age1_rubble",
    },
    Item {
        building: "outpost",
        role: "standing",
        stem: "b_dark_outpost_age1",
    },
    Item {
        building: "outpost",
        role: "destruction",
        stem: "b_dark_outpost_age1_destruction",
    },
    Item {
        building: "outpost",
        role: "rubble",
        stem: "b_dark_outpost_age1_rubble",
    },
    Item {
        building: "dock",
        role: "standing",
        stem: "b_dark_dock_age1",
    },
    Item {
        building: "dock",
        role: "destruction",
        stem: "b_dark_dock_age1_destruction",
    },
    Item {
        building: "dock",
        role: "rubble",
        stem: "b_dark_dock_age1_rubble",
    },
    Item {
        building: "town_center",
        role: "standing",
        stem: "b_dark_town_center_age1",
    },
    Item {
        building: "town_center",
        role: "main",
        stem: "b_dark_town_center_age1_main",
    },
    Item {
        building: "town_center",
        role: "back",
        stem: "b_dark_town_center_age1_back",
    },
    Item {
        building: "town_center",
        role: "center",
        stem: "b_dark_town_center_age1_center",
    },
    Item {
        building: "town_center",
        role: "front",
        stem: "b_dark_town_center_age1_front",
    },
    Item {
        building: "town_center",
        role: "destruction",
        stem: "b_dark_town_center_age1_destruction",
    },
    Item {
        building: "town_center",
        role: "rubble",
        stem: "b_dark_town_center_age1_rubble",
    },
    Item {
        building: "palisade",
        role: "standing",
        stem: "b_dark_wall_palisade",
    },
    Item {
        building: "palisade",
        role: "destruction",
        stem: "b_dark_wall_palisade_destruction",
    },
    Item {
        building: "palisade",
        role: "rubble",
        stem: "b_dark_wall_palisade_rubble",
    },
    Item {
        building: "lumber_camp",
        role: "standing",
        stem: "b_west_lumber_camp_age2",
    },
    Item {
        building: "lumber_camp",
        role: "destruction",
        stem: "b_west_lumber_camp_age2_destruction",
    },
    Item {
        building: "lumber_camp",
        role: "rubble",
        stem: "b_west_lumber_camp_age2_rubble",
    },
    Item {
        building: "mining_camp",
        role: "standing",
        stem: "b_west_mining_camp_age2",
    },
    Item {
        building: "mining_camp",
        role: "destruction",
        stem: "b_west_mining_camp_age2_destruction",
    },
    Item {
        building: "mining_camp",
        role: "rubble",
        stem: "b_west_mining_camp_age2_rubble",
    },
    Item {
        building: "foundation_1x1",
        role: "foundation",
        stem: "b_misc_foundation_1x1",
    },
    Item {
        building: "foundation_2x2",
        role: "foundation",
        stem: "b_misc_foundation_2x2",
    },
    Item {
        building: "foundation_3x3",
        role: "foundation",
        stem: "b_misc_foundation_3x3",
    },
    Item {
        building: "foundation_4x4",
        role: "foundation",
        stem: "b_misc_foundation_4x4",
    },
    Item {
        building: "foundation_5x5",
        role: "foundation",
        stem: "b_misc_foundation_5x5",
    },
    Item {
        building: "foundation_house",
        role: "foundation",
        stem: "b_misc_foundation_house",
    },
    Item {
        building: "foundation_mill",
        role: "foundation",
        stem: "b_misc_foundation_mill",
    },
    Item {
        building: "foundation_barracks",
        role: "foundation",
        stem: "b_misc_foundation_barracks",
    },
    Item {
        building: "foundation_outpost",
        role: "foundation",
        stem: "b_misc_foundation_outpost",
    },
    Item {
        building: "foundation_dock",
        role: "foundation",
        stem: "b_misc_foundation_dock",
    },
    Item {
        building: "foundation_town_center",
        role: "foundation",
        stem: "b_misc_foundation_town_center",
    },
    Item {
        building: "foundation_lumber_camp",
        role: "foundation",
        stem: "b_misc_foundation_lumber_camp",
    },
    Item {
        building: "foundation_mining_camp",
        role: "foundation",
        stem: "b_misc_foundation_mining_camp",
    },
    Item {
        building: "rubble",
        role: "rubble",
        stem: "b_misc_rubble",
    },
    Item {
        building: "rubble",
        role: "rubble_alternative",
        stem: "b_misc_rubble_alternative",
    },
];
