//! Synthetic `empires2.dat` fixtures: structure and bounds only, no game data.
use super::*;
use flate2::{Compression, write::DeflateEncoder};
use std::io::Write;

#[path = "tests/bridges.rs"]
mod bridges;
mod composition;
#[path = "tests/corpse.rs"]
mod corpse_tests;
#[path = "tests/fishing.rs"]
mod fishing_tests;
mod interface;
mod inventory;
mod trees;
fn le16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn padded(out: &mut Vec<u8>, text: &str, width: usize) {
    let mut field = text.as_bytes().to_vec();
    field.resize(width, 0);
    out.extend_from_slice(&field);
}

fn graphic(
    out: &mut Vec<u8>,
    id: i16,
    name: &str,
    slp: i32,
    frames: u16,
    angles: u16,
    deltas: &[i16],
) {
    padded(out, name, 21);
    padded(out, name, 13);
    out.extend_from_slice(&slp.to_le_bytes());
    out.extend_from_slice(&[0; 3 + 2 + 1 + 8]);
    out.extend_from_slice(&u16::try_from(deltas.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&[0xff, 0xff, 0]);
    out.extend_from_slice(&frames.to_le_bytes());
    out.extend_from_slice(&angles.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&0.5f32.to_le_bytes());
    out.extend_from_slice(&[0; 4 + 1]);
    le16(out, id);
    out.extend_from_slice(&[0; 2]);
    for delta in deltas {
        le16(out, *delta);
        out.extend_from_slice(&[0; 14]);
    }
}

struct Spec {
    kind: u8,
    id: i16,
    name: &'static str,
    stand: i16,
    walk: i16,
    attack: i16,
    sprites: u8,
    construction: i16,
}

fn unit(out: &mut Vec<u8>, spec: &Spec) {
    let start = out.len();
    out.push(spec.kind);
    out.extend_from_slice(&u16::try_from(spec.name.len() + 1).unwrap().to_le_bytes());
    le16(out, spec.id);
    out.extend_from_slice(&[0; 6]);
    le16(out, spec.stand);
    le16(out, -1);
    le16(out, 7); // dying
    out.extend_from_slice(&[0; 3]);
    le16(out, 25); // hit points
    out.extend_from_slice(&4.0f32.to_le_bytes());
    out.resize(start + 145, 0);
    out[start + 43..start + 45].copy_from_slice(&(-1i16).to_le_bytes());
    out[start + 27..start + 31].copy_from_slice(&1.5f32.to_le_bytes());
    out[start + 31..start + 35].copy_from_slice(&1.5f32.to_le_bytes());
    out[start + 75..start + 77].copy_from_slice(&15i16.to_le_bytes());
    out[start + 124..start + 126].copy_from_slice(&0i16.to_le_bytes());
    out[start + 126..start + 130].copy_from_slice(&225f32.to_le_bytes());
    out.push(spec.sprites);
    out.extend_from_slice(&vec![0; usize::from(spec.sprites) * 5 + 6]);
    padded(out, spec.name, spec.name.len() + 1);
    out.extend_from_slice(&[0; 4]);
    if spec.kind >= 20 {
        out.extend_from_slice(&0.8f32.to_le_bytes());
    }
    if spec.kind >= 30 {
        le16(out, spec.walk);
        le16(out, -1);
        out.extend_from_slice(&[0; 33]);
    }
    if spec.kind >= 40 {
        out.extend_from_slice(&[0; 2]);
        out.extend_from_slice(&4f32.to_le_bytes());
        out.extend_from_slice(&0.49f32.to_le_bytes());
        out.extend_from_slice(&[0; 10]);
    }
    if spec.kind >= 50 {
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        le16(out, 4);
        le16(out, 3); // melee 3
        out.extend_from_slice(&1u16.to_le_bytes());
        le16(out, 3);
        le16(out, 1); // pierce 1
        out.extend_from_slice(&[0; 42]);
        le16(out, spec.attack);
        out.extend_from_slice(&[0; 12]);
    }
    if spec.kind >= 70 {
        for cost in [(0i16, 50i16, 1i16), (-1, 0, 0), (4, 1, 0)] {
            le16(out, cost.0);
            le16(out, cost.1);
            le16(out, cost.2);
        }
        le16(out, 25); // train time
        le16(out, 109);
        out.extend_from_slice(&[0; 43]);
    }
    if spec.kind == 80 {
        let building_start = out.len();
        le16(out, spec.construction);
        out.resize(building_start + 80, 0);
        for slot in 0..4 {
            out[building_start + 15 + slot * 10..building_start + 17 + slot * 10]
                .copy_from_slice(&(-1i16).to_le_bytes());
        }
    }
}

fn task(out: &mut Vec<u8>, graphics: [i16; 4]) {
    le16(out, 1);
    le16(out, 0);
    out.push(0);
    for value in [5i16, -1, -1, -1, 1, -1, 0, -1] {
        le16(out, value);
    }
    out.extend_from_slice(&[0; 26]);
    for g in graphics {
        le16(out, g);
    }
    out.extend_from_slice(&[0; 4]);
}

fn fixture(terrain_filler: usize) -> Vec<u8> {
    fixture_with(terrain_filler, 2)
}

/// The villager occupies unit slot `slot` (at least 2); other slots are empty.
fn fixture_with(terrain_filler: usize, slot: usize) -> Vec<u8> {
    let slots = slot + 1;
    let mut out = b"VER 5.7\0".to_vec();
    out.extend_from_slice(&[1, 0, 2, 0]); // one restriction, two terrains
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&[0; 2 * 20]);
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&[0; 36]);
    out.extend_from_slice(&[1, 0]); // one sound with one file
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&[0; 4 + 23]);
    out.extend_from_slice(&5u16.to_le_bytes());
    for present in [1i32, 0, 1, 1, 1] {
        out.extend_from_slice(&present.to_le_bytes());
    }
    graphic(&mut out, 0, "", 0, 0, 0, &[]);
    graphic(&mut out, 2, "HOUS1NNG", 2223, 3, 1, &[]);
    graphic(&mut out, 3, "HOUS1N0G", 2221, 1, 1, &[2]);
    graphic(&mut out, 4, "VMBAS_FN", 1479, 15, 8, &[]);
    // AoK terrain block: 24-byte map header, 19 six-byte tile sizes,
    // two-byte alignment, then 32 terrain records of 416 bytes.
    out.extend_from_slice(&[0; 140]);
    for id in 0..32 {
        let start = out.len();
        out.extend_from_slice(&[1, 0]);
        padded(&mut out, "Synthetic", 13);
        padded(&mut out, "SYNTH", 13);
        out.extend_from_slice(&(15000i32 + id).to_le_bytes());
        out.resize(start + 416, 0);
        out[start + 192..start + 194].copy_from_slice(&(-1i16).to_le_bytes());
        out[start + 194..start + 196].copy_from_slice(&10i16.to_le_bytes());
        out[start + 196..start + 198].copy_from_slice(&10i16.to_le_bytes());
    }
    out.extend_from_slice(&vec![0xAA; terrain_filler]);
    out.extend_from_slice(&u32::try_from(slots).unwrap().to_le_bytes()); // unit headers
    out.push(1);
    out.extend_from_slice(&2u16.to_le_bytes());
    task(&mut out, [-1, 4, -1, 3]);
    task(&mut out, [-1, -1, -1, -1]);
    out.extend_from_slice(&vec![0; slot - 1]);
    out.push(1);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[1, 0, 1]); // civilisation block
    padded(&mut out, "Gaia", 20);
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&[0; 8]);
    out.push(0);
    out.extend_from_slice(&u16::try_from(slots).unwrap().to_le_bytes());
    for at in 0..slots {
        let present = i32::from(at == 0 || at == slot);
        out.extend_from_slice(&present.to_le_bytes());
    }
    let house = Spec {
        kind: 80,
        id: 0,
        name: "HOUS",
        stand: 2,
        walk: -1,
        attack: -1,
        sprites: 2,
        construction: 4,
    };
    unit(&mut out, &house);
    let villager = Spec {
        kind: 70,
        id: i16::try_from(slot).unwrap(),
        name: "VMBAS",
        stand: 4,
        walk: 4,
        attack: 3,
        sprites: 0,
        construction: -1,
    };
    unit(&mut out, &villager);
    out
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn fixture_decodes_graphics_units_and_tasks() {
    let file = decode(&deflate(&fixture(37))).unwrap();
    assert_eq!(file.version, "VER 5.7");
    assert_eq!(
        file.graphics.iter().map(|g| g.id).collect::<Vec<_>>(),
        [0, 2, 3, 4]
    );
    let hous = file.graphic(2).unwrap();
    assert_eq!(
        (hous.slp_id, hous.frame_count, hous.angle_count),
        (2223, 3, 1)
    );
    assert_eq!(file.graphic(3).unwrap().deltas[0].graphic_id, 2);
    assert_eq!(file.report.stopped, None);
    assert_eq!(file.report.civilisation_name.as_deref(), Some("Gaia"));
    assert_eq!((file.report.unit_slots, file.report.units_parsed), (3, 2));
    let house = file.unit(0).unwrap();
    assert_eq!(
        (house.kind, house.name.as_str(), house.standing[0]),
        (80, "HOUS", 2)
    );
    assert_eq!(house.construction_graphic, Some(4));
    let villager = file.unit(2).unwrap();
    assert_eq!(villager.walking, Some([4, -1]));
    assert_eq!(villager.attack_graphic, Some(3));
    assert_eq!(villager.dead_unit, -1);
    assert_eq!(
        (villager.attacks.clone(), villager.armours.clone()),
        (vec![(4, 3)], vec![(3, 1)])
    );
    assert_eq!(villager.train_time, Some(25));
    assert_eq!(villager.costs[0].amount, 50);
    assert_eq!(villager.collision_size, [1.5, 1.5]);
    assert_eq!(villager.resource_capacity, 15);
    assert_eq!(villager.resource_storage[0], (0, 225.0));
    assert_eq!(villager.work_rate, Some(0.49));
    assert!(villager.tasks.is_empty());
    assert_eq!(file.unit(0).unwrap().tasks.len(), 2);
    assert_eq!(file.unit(0).unwrap().tasks[0].graphics, [-1, 4, -1, 3]);
}

#[test]
fn headers_are_found_whatever_filler_precedes_them() {
    for filler in [0, 1, 4, 101] {
        let file = parse(&fixture(filler)).unwrap();
        assert_eq!(file.report.stopped, None, "filler {filler}");
        assert_eq!(file.unit(0).unwrap().tasks.len(), 2, "filler {filler}");
    }
}

#[test]
fn table_flags_units_the_dat_lacks_and_pack_gaps() {
    let file = parse(&fixture(9)).unwrap();
    let pack: std::collections::BTreeSet<i32> = [2223, 1479].into();
    let table = table::build(&file, Some(&pack));
    let militia = table.rows.iter().find(|r| r.label == "Militia").unwrap();
    assert_eq!(
        (militia.pack.as_str(), militia.internal_name.clone()),
        ("absent", None)
    );
    assert!(
        table.checks.iter().all(|c| c.status == "mismatch"),
        "fixture has no reviewed ids"
    );
    let ids = table::slp_ids(&[0u8; 8]);
    assert!(ids.is_err(), "a non-DRS buffer is rejected, not read");
}

#[test]
fn unsupported_version_and_bad_counts_are_rejected() {
    let mut other = fixture(0);
    other[4..7].copy_from_slice(b"9.9");
    assert!(matches!(parse(&other), Err(Error::Unsupported { .. })));
    let mut huge = fixture(0);
    huge[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(matches!(parse(&huge), Err(Error::Format { .. })));
    let mut huge_terrains = fixture(0);
    huge_terrains[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(parse(&huge_terrains).is_err());
}

#[test]
fn truncation_never_panics() {
    let whole = fixture(5);
    for len in (0..whole.len()).step_by(7) {
        let _ = parse(&whole[..len]);
    }
}

#[test]
fn mismatched_graphic_slot_is_an_error() {
    let mut bytes = fixture(0);
    let at = bytes.windows(8).position(|w| w == b"HOUS1NNG").unwrap()
        + 21
        + 13
        + 4
        + 3
        + 2
        + 1
        + 8
        + 2
        + 2
        + 1
        + 2
        + 2
        + 4
        + 4
        + 4
        + 1;
    bytes[at..at + 2].copy_from_slice(&9i16.to_le_bytes());
    assert!(matches!(parse(&bytes), Err(Error::Format { .. })));
}

#[test]
fn a_later_stage_failure_keeps_the_graphics() {
    let mut bytes = fixture(0);
    let at = bytes.windows(4).position(|w| w == b"Gaia").unwrap() - 3;
    bytes[at + 2] = 0; // civilisation marker destroyed
    let file = parse(&bytes).unwrap();
    assert_eq!(file.graphics.len(), 4);
    assert!(file.units.is_empty());
    assert!(
        file.report
            .stopped
            .as_deref()
            .unwrap()
            .contains("civilisation")
    );
}

#[test]
fn inflation_is_bounded() {
    let bomb = deflate(&vec![0u8; 65 * 1024 * 1024]);
    assert!(matches!(decode(&bomb), Err(Error::Format { .. })));
}

#[test]
fn a_matching_graphic_is_reported_as_a_match_and_a_wrong_role_is_not() {
    let file = parse(&fixture_with(3, 83)).unwrap();
    assert_eq!(file.report.stopped, None);
    let table = table::build(&file, None);
    let status = |label: &str| table.checks.iter().find(|c| c.label == label).unwrap();
    // The fixture villager (unit 83) stands on SLP 1479 and walks on the same
    // sprite, so only the idle check can match; walk expects SLP 1484.
    assert_eq!(status("Villager idle").status, "match");
    assert!(
        status("Villager idle")
            .dat_says
            .iter()
            .any(|line| line.contains("unit 83 VMBAS stand"))
    );
    assert_eq!(status("Villager walk").status, "mismatch");
    assert_eq!(status("Town Center (Dark Age main)").status, "mismatch");
    let row = table.rows.iter().find(|r| r.unit == 83).unwrap();
    assert_eq!(row.graphics[0].slp, 1479);
    assert_eq!(
        row.line_of_sight,
        Some(file.unit(83).unwrap().line_of_sight)
    );
}

#[test]
fn missing_unit_headers_are_a_stop_reason_not_silence() {
    let good = fixture(0);
    let offset = parse(&good).unwrap().report.unit_headers_offset.unwrap();
    let mut shifted = good.clone();
    shifted[offset + 5] = 3; // first header claims three tasks instead of two
    let file = parse(&shifted).unwrap();
    let reason = file.report.stopped.as_deref().unwrap();
    assert!(reason.contains("unit headers not found"), "{reason}");
    assert_eq!(file.report.unit_headers_offset, None);
    assert_eq!(file.units.len(), 2, "units still decode without tasks");
    assert!(file.unit(0).unwrap().tasks.is_empty());
    let table = table::build(&file, None);
    assert!(table.status.starts_with("INCOMPLETE"));
    assert!(table::markdown(&table).contains("INCOMPLETE"));
}

#[test]
fn missing_gameplay_graphics_make_a_unit_partial() {
    let mut file = parse(&fixture_with(0, 83)).unwrap();
    let pack: std::collections::BTreeSet<i32> = [1479].into();
    let pack_of = |f: &DatFile| {
        let table = table::build(f, Some(&pack));
        let row = table.rows.iter().find(|r| r.unit == 83).unwrap();
        (row.pack.clone(), row.graphics.len())
    };
    file.units[1].attack_graphic = Some(4);
    assert_eq!(pack_of(&file).0, "complete");
    // Graphic 3 is SLP 2221, absent from this synthetic pack.
    file.units[1].attack_graphic = Some(3);
    assert_eq!(pack_of(&file).0, "partial");
}
