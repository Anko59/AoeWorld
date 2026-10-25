//! Shared supplemental art selection for the browser and private atlas proof.
//! Native DAT parts take priority. These slots fill absent native art only.
pub struct Selection {
    pub slot: usize,
    pub source: String,
    pub frames: u32,
    pub step: usize,
    pub take: usize,
    pub resident: bool,
    pub required: bool,
}

/// Bridges have no owner-colour pixels and can borrow spare terrain space.
/// Buildings retain aligned team-mask pages; farm ground stays on terrain.
pub fn domain(slot: usize) -> super::packing::AtlasDomain {
    use super::packing::AtlasDomain;
    match slot {
        5 => AtlasDomain::Terrain,
        65..=70 => AtlasDomain::Shared,
        _ => AtlasDomain::Objects,
    }
}

fn trial(id: u32) -> String {
    format!("graphics.drs:[32, 112, 108, 115]:{id}")
}

pub fn supplemental(
    present: impl Fn(&str) -> bool,
    native: impl Fn(i32) -> bool,
    stages: impl Fn(f32) -> bool,
    construction: impl Fn(i32) -> bool,
) -> Vec<Selection> {
    let mut out = Vec::new();
    let stems = [
        "",
        "b_dark_house_age1",
        "b_west_lumber_camp_age2",
        "b_west_mining_camp_age2",
        "b_dark_mill_age1",
        "",
        "b_dark_barracks_age1",
        "b_dark_wall_palisade",
        "b_dark_outpost_age1",
    ];
    for slot in 0..26 {
        if slot == 0
            || slot == 4
            || slot < 9 && slot != 5 && native([109, 70, 562, 584, 68, 50, 12, 72, 598][slot])
            || slot == 23 && native(45)
            || slot == 24 && construction(45)
            || (9..=12).contains(&slot) && stages((slot - 8) as f32)
        {
            continue;
        }
        let (mut source, mut frames, mut step) = match slot {
            5 => ("de/g_fm1.dds".into(), 1, 1),
            1..=8 => (
                format!("de/{}_x1.sld", stems[slot]),
                if slot == 1 {
                    3
                } else if slot == 7 {
                    5
                } else {
                    1
                },
                1,
            ),
            9..=12 => (
                format!("de/b_misc_foundation_{}x{}_x1.sld", slot - 8, slot - 8),
                3,
                1,
            ),
            13..=21 => {
                let kind = slot - 13;
                let stem = if kind == 0 {
                    "b_dark_town_center_age1"
                } else if kind == 5 || kind == 7 {
                    "b_misc"
                } else {
                    stems[kind]
                };
                (format!("de/{stem}_rubble_x1.sld"), 1, 1)
            }
            22 => (trial(1496), 75, 1),
            23 => ("de/b_dark_dock_age1_x1.sld".into(), 1, 1),
            24 => ("de/b_misc_foundation_dock_x1.sld".into(), 3, 1),
            _ => ("de/b_misc_rubble_x1.sld".into(), 1, 1),
        };
        if !present(&source) {
            let fallback = match slot {
                1 => Some((2223, 3, 1)),
                6 => Some((2683, 1, 1)),
                7 => Some((4534, 45, 9)),
                8 => Some((4330, 1, 1)),
                23 => Some((376, 1, 1)),
                24 => Some((4397, 3, 1)),
                9..=12 => Some((236 + slot as u32 - 9, 3, 1)),
                _ => None,
            };
            if let Some((id, count, stride)) = fallback {
                source = trial(id);
                frames = count;
                step = stride;
            }
        }
        out.push(Selection {
            slot,
            source,
            frames,
            step,
            take: usize::MAX,
            resident: true,
            required: false,
        });
    }
    if !native(68) && (present(&trial(3483)) || present(&trial(3482))) {
        for (id, frames) in [(3483, 1), (3482, 43)] {
            out.push(Selection {
                slot: 4,
                source: trial(id),
                frames,
                step: 1,
                take: usize::MAX,
                resident: true,
                required: true,
            });
        }
    }
    let layered_mill = present(&trial(741)) || present(&trial(737));
    for (slot, id, stem, count, fallback) in [
        (26, 87, "b_west_archery_range_age2", 1, Some((24, 1))),
        (27, 101, "b_west_stable_age2", 90, Some((1009, 1))),
        (28, 103, "b_west_blacksmith_age2", 1, None),
        (29, 84, "b_west_market_age2", 1, Some((2278, 1))),
        (30, 79, "b_west_tower_age2", 1, Some((2655, 1))),
        (31, 792, "b_dark_gate_palisade_se_closed", 1, None),
        (32, -1, "b_west_archery_range_age2_rubble", 1, None),
        (33, -1, "b_west_stable_age2_rubble", 1, None),
        (34, -1, "b_west_blacksmith_age2_rubble", 1, None),
        (35, -1, "b_west_market_age2_rubble", 1, None),
        (36, -1, "b_west_tower_age2_rubble", 1, None),
        (37, -1, "b_dark_gate_palisade_se_rubble", 1, None),
        (38, 71, "b_west_town_center_age2", 1, None),
        (39, 463, "b_west_house_age2", 3, Some((2235, 3))),
        (40, 129, "b_west_mill_age2", 91, None),
        (41, 498, "b_west_barracks_age2", 1, Some((133, 1))),
        (42, -1, "b_dark_gate_palisade_se_open", 1, None),
        (43, -1, "b_dark_gate_palisade_se_constr", 3, None),
        (44, 133, "b_west_dock_age2", 1, Some((388, 1))),
        (65, -1, "s_bridge_a_top", 1, Some((3688, 1))),
        (66, -1, "s_bridge_a_middle", 1, Some((3690, 1))),
        (67, -1, "s_bridge_a_bottom", 1, Some((3692, 1))),
        (68, -1, "s_bridge_b_top", 1, Some((3694, 1))),
        (69, -1, "s_bridge_b_middle", 1, Some((3696, 1))),
        (70, -1, "s_bridge_b_bottom", 1, Some((3698, 1))),
    ] {
        let mut source = format!("de/{stem}_x1.sld");
        let mut frames = count;
        if (!present(&source) || slot >= 45 && fallback.is_some_and(|(id, _)| present(&trial(id))))
            && let Some((id, count)) = fallback
        {
            source = trial(id);
            frames = count;
        }
        out.push(Selection {
            slot,
            source,
            frames,
            step: 1,
            take: if slot == 40 || slot == 39 || slot == 43 {
                usize::MAX
            } else {
                1
            },
            resident: !(id >= 0 && native(id) || slot == 40 && layered_mill),
            required: false,
        });
    }
    if !native(129) && layered_mill {
        for (id, frames) in [(741, 1), (737, 8)] {
            out.push(Selection {
                slot: 40,
                source: trial(id),
                frames,
                step: 1,
                take: usize::MAX,
                resident: true,
                required: true,
            });
        }
    }
    for (i, (id, slp, count)) in [(82, 305, 1), (104, 281, 1), (209, 1363, 9), (49, 958, 1)]
        .into_iter()
        .enumerate()
    {
        out.push(Selection {
            slot: 45 + i,
            source: if id == 82 && !present(&trial(slp)) {
                "de/b_west_castle_age3_x1.sld".into()
            } else {
                trial(slp)
            },
            frames: count,
            step: 1,
            take: usize::MAX,
            resident: !native(id),
            required: false,
        });
        out.push(Selection {
            slot: 49 + i,
            source: if id == 82 && !present(&trial(931)) {
                "de/b_west_castle_age3_rubble_x1.sld".into()
            } else {
                trial(if id == 104 { 930 } else { 931 })
            },
            frames: 1,
            step: 1,
            take: 1,
            resident: true,
            required: false,
        });
    }
    for (i, (id, slp, count)) in [
        (141, 3620, 1),
        (464, 2247, 3),
        (564, 3503, 1),
        (586, 3491, 1),
        (130, 749, 10),
        (132, 145, 1),
        (47, 2263, 60),
        (10, 36, 1),
        (86, 1021, 1),
        (18, 105, 1),
        (116, 820, 1),
        (31, 281, 1),
    ]
    .into_iter()
    .enumerate()
    {
        let source = if !present(&trial(slp)) && matches!(id, 564 | 586) {
            format!(
                "de/b_west_{}_camp_age2_x1.sld",
                if id == 564 { "lumber" } else { "mining" }
            )
        } else {
            trial(slp)
        };
        let (source, frames) = if id == 47 && !present(&trial(slp)) {
            ("de/b_west_dock_age3_x1.sld".into(), 1)
        } else {
            (source, count)
        };
        out.push(Selection {
            slot: 53 + i,
            source,
            frames,
            step: 1,
            take: usize::MAX,
            resident: !native(id),
            required: false,
        });
    }
    out
}
