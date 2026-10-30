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
        building: "projectile",
        role: "stone",
        stem: "p_mangonel",
    },
    Item {
        building: "rally_point",
        role: "flag",
        stem: "b_misc_waypoint_flag",
    },
];
