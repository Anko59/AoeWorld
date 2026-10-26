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
        building: "castle_dock",
        role: "standing",
        stem: "b_west_dock_age3",
    },
    Item {
        building: "bridge_a",
        role: "top",
        stem: "s_bridge_a_top",
    },
    Item {
        building: "bridge_a",
        role: "middle",
        stem: "s_bridge_a_middle",
    },
    Item {
        building: "bridge_a",
        role: "bottom",
        stem: "s_bridge_a_bottom",
    },
    Item {
        building: "bridge_b",
        role: "top",
        stem: "s_bridge_b_top",
    },
    Item {
        building: "bridge_b",
        role: "middle",
        stem: "s_bridge_b_middle",
    },
    Item {
        building: "bridge_b",
        role: "bottom",
        stem: "s_bridge_b_bottom",
    },
    Item {
        building: "galley",
        role: "standing",
        stem: "u_shp_galley",
    },
    Item {
        building: "war_galley",
        role: "standing",
        stem: "u_shp_war_galley",
    },
    Item {
        building: "fire_ship",
        role: "standing",
        stem: "u_shp_fire_ship",
    },
    Item {
        building: "demolition_ship",
        role: "standing",
        stem: "u_shp_demo_ship",
    },
    Item {
        building: "transport_ship",
        role: "standing",
        stem: "u_shp_transport_ship",
    },
    Item {
        building: "rally_point",
        role: "flag",
        stem: "b_misc_waypoint_flag",
    },
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
    Item {
        building: "archery_range",
        role: "standing",
        stem: "b_west_archery_range_age2",
    },
    Item {
        building: "archery_range",
        role: "rubble",
        stem: "b_west_archery_range_age2_rubble",
    },
    Item {
        building: "stable",
        role: "standing",
        stem: "b_west_stable_age2",
    },
    Item {
        building: "stable",
        role: "rubble",
        stem: "b_west_stable_age2_rubble",
    },
    Item {
        building: "blacksmith",
        role: "standing",
        stem: "b_west_blacksmith_age2",
    },
    Item {
        building: "blacksmith",
        role: "rubble",
        stem: "b_west_blacksmith_age2_rubble",
    },
    Item {
        building: "market",
        role: "standing",
        stem: "b_west_market_age2",
    },
    Item {
        building: "market",
        role: "rubble",
        stem: "b_west_market_age2_rubble",
    },
    Item {
        building: "watch_tower",
        role: "standing",
        stem: "b_west_tower_age2",
    },
    Item {
        building: "watch_tower",
        role: "rubble",
        stem: "b_west_tower_age2_rubble",
    },
    Item {
        building: "feudal_dock",
        role: "standing",
        stem: "b_west_dock_age2",
    },
    Item {
        building: "feudal_house",
        role: "standing",
        stem: "b_west_house_age2",
    },
    Item {
        building: "feudal_house",
        role: "rubble",
        stem: "b_west_house_age2_rubble",
    },
    Item {
        building: "feudal_mill",
        role: "standing",
        stem: "b_west_mill_age2",
    },
    Item {
        building: "feudal_mill",
        role: "rubble",
        stem: "b_west_mill_age2_rubble",
    },
    Item {
        building: "feudal_barracks",
        role: "standing",
        stem: "b_west_barracks_age2",
    },
    Item {
        building: "feudal_barracks",
        role: "rubble",
        stem: "b_west_barracks_age2_rubble",
    },
    Item {
        building: "feudal_town_center",
        role: "standing",
        stem: "b_west_town_center_age2",
    },
    Item {
        building: "feudal_town_center",
        role: "rubble",
        stem: "b_west_town_center_age2_rubble",
    },
    Item {
        building: "palisade_gate",
        role: "standing",
        stem: "b_dark_gate_palisade_se_closed",
    },
    Item {
        building: "palisade_gate",
        role: "open",
        stem: "b_dark_gate_palisade_se_open",
    },
    Item {
        building: "palisade_gate",
        role: "rubble",
        stem: "b_dark_gate_palisade_se_rubble",
    },
    Item {
        building: "palisade_gate",
        role: "foundation",
        stem: "b_dark_gate_palisade_se_constr",
    },
];
