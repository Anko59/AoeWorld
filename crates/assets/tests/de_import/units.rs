//! Synthetic common DAT unit records built from the semantic action contract.
use aoe_assets::{
    catalog::units::SPRITES,
    de::unit_catalog::{Action, BINDINGS},
};
use std::collections::BTreeMap;
pub(super) fn append(bytes: &mut Vec<u8>) {
    let mut units: BTreeMap<i16, [i16; 8]> = BTreeMap::new();
    for b in BINDINGS {
        let graphic = (SPRITES[b.pose as usize].id - 200_000) as i16;
        let index = match b.action {
            Action::Idle => 0,
            Action::Walk => 1,
            Action::Run => 2,
            Action::Attack => 3,
            Action::Death => 4,
            Action::Decay => 5,
            Action::Work => 6,
            Action::Carry => 7,
        };
        units.entry(b.unit).or_insert([-1; 8])[index] = graphic;
    }
    for (id, g) in units {
        let dead = if g[5] >= 0 && g[5] != g[4] {
            id + 3000
        } else {
            -1
        };
        bytes.extend(record(id, &g, dead));
        if dead >= 0 {
            bytes.extend(record(dead, &[g[5], -1, -1, -1, -1, -1, -1, -1], -1));
        }
    }
}
pub(super) fn record(id: i16, g: &[i16; 8], dead: i16) -> Vec<u8> {
    let mut row = vec![0; 180];
    row[0] = 70;
    row[1..3].copy_from_slice(&id.to_le_bytes());
    row[13..15].copy_from_slice(&g[0].to_le_bytes());
    row[17..19].copy_from_slice(&g[4].to_le_bytes());
    row[45..47].copy_from_slice(&dead.to_le_bytes());
    row.extend_from_slice(&[0x60, 0x0a, 4, 0]);
    row.extend(b"TEST");
    row.extend(id.to_le_bytes());
    row.extend(id.to_le_bytes());
    row.extend([0; 4]);
    row.extend(g[1].to_le_bytes());
    row.extend(g[2].to_le_bytes());
    row.extend([0; 37 + 10 + 2 + 14]);
    row.extend(1_u16.to_le_bytes());
    let mut task = [0; 69];
    task[49..51].copy_from_slice(&(-1_i16).to_le_bytes());
    task[51..53].copy_from_slice(&g[6].to_le_bytes());
    task[53..55].copy_from_slice(&g[7].to_le_bytes());
    row.extend(task);
    row.extend([0; 2 + 2 + 2 + 6 + 12 + 2 + 3]);
    row.extend(0_i16.to_le_bytes());
    row.extend([0; 13 + 4 + 4]);
    row.extend(g[3].to_le_bytes());
    row
}
