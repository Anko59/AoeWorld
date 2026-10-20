//! Synthetic numeric records; no original game data is embedded.
use super::*;

fn string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&0x0a60_u16.to_le_bytes());
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}
fn unit(id: i16, language: i32, name: &str) -> Vec<u8> {
    let mut out = vec![0; 136 + 21 + 1 + 22];
    out[0] = 80;
    out[1..3].copy_from_slice(&id.to_le_bytes());
    out[3..7].copy_from_slice(&language.to_le_bytes());
    out[22..24].copy_from_slice(&1234_i16.to_le_bytes());
    out[24..28].copy_from_slice(&6_f32.to_le_bytes());
    out[29..33].copy_from_slice(&1.5_f32.to_le_bytes());
    out[33..37].copy_from_slice(&1.5_f32.to_le_bytes());
    string(&mut out, name);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&[0; 4 + 41 + 10 + 2 + 14 + 2 + 2]); // no drops/tasks; default armour
    for pairs in [&[(3_i16, 7_i16)][..], &[(3_i16, 9_i16), (4, -2)][..]] {
        out.extend_from_slice(&(pairs.len() as u16).to_le_bytes());
        for (a, b) in pairs {
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
    }
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&8_f32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&2_f32.to_le_bytes());
    out.extend_from_slice(&[0; 20]);
    out.extend_from_slice(&1_f32.to_le_bytes());
    out.extend_from_slice(&[0; 38]);
    for (kind, amount, enabled) in [(1_i16, 42_i16, 1_i16), (-1, 0, 0), (-1, 0, 0)] {
        for n in [kind, amount, enabled] {
            out.extend_from_slice(&n.to_le_bytes());
        }
    }
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&17_i16.to_le_bytes());
    out.extend_from_slice(&[0; 7]);
    out
}
fn tech(language: i32, effect: i16) -> Vec<u8> {
    let mut out = vec![0; 55];
    out[33..37].copy_from_slice(&language.to_le_bytes());
    out[37..41].copy_from_slice(&(language + 1000).to_le_bytes());
    out[41..43].copy_from_slice(&effect.to_le_bytes());
    string(&mut out, "synthetic research");
    out.push(1);
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&42_i16.to_le_bytes());
    out.extend_from_slice(&17_i16.to_le_bytes());
    out.push(1);
    out.extend_from_slice(&123_i32.to_le_bytes());
    out
}
fn fixture() -> Vec<u8> {
    let mut out = b"VER 8.9\0".to_vec();
    out.extend_from_slice(&300_i32.to_le_bytes());
    for id in 0..300 {
        string(
            &mut out,
            if id == 1 {
                "synthetic palisade"
            } else {
                "synthetic effect"
            },
        );
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.push(5);
        for n in [1_i16, 2, 3] {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out.extend_from_slice(&1.25_f32.to_le_bytes());
    }
    for &(id, language, name) in BUILDINGS {
        out.extend(unit(
            id,
            language,
            if name.is_empty() {
                "synthetic building"
            } else {
                name
            },
        ));
    }
    for (i, &(_, language)) in TECHS.iter().enumerate() {
        out.extend(tech(language, i as i16));
    }
    out
}

#[test]
fn complete_synthetic_de_survey_preserves_numeric_records_and_source_identity() {
    use std::io::Write;
    let bytes = fixture();
    let report = survey(&bytes).unwrap();
    assert_eq!(report.buildings.len(), BUILDINGS.len());
    assert_eq!(report.technologies.len(), TECHS.len());
    let b = &report.buildings[0];
    assert_eq!(
        (
            b.hp,
            b.los,
            b.collision,
            b.range,
            b.min_range,
            b.reload,
            b.build_seconds
        ),
        (1234, 6., [1.5, 1.5], 8., 1., 2., 17)
    );
    assert_eq!(b.armours, vec![(3, 9), (4, -2)]);
    assert_eq!(b.cost[0], (1, 42, 1));
    assert_eq!(report.technologies[0].locations, vec![(42, 17, 1, 123)]);
    assert_eq!(report.effects[0].1.commands, vec![(5, 1, 2, 3, 1.25)]);
    let mut compressed =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    compressed.write_all(&bytes).unwrap();
    let compressed = compressed.finish().unwrap();
    let report = decode(&compressed).unwrap();
    assert_eq!(
        report.source_hash,
        blake3::hash(&compressed).to_hex().to_string()
    );
    assert_eq!(report.inflated_bytes, bytes.len());
}

#[test]
fn synthetic_de_numeric_record_rejects_every_truncation_and_bad_identity() {
    let bytes = unit(87, 5128, "fixture");
    assert!(unit_record(&bytes, 0, 87, "fixture").is_ok());
    for n in 0..bytes.len() {
        assert!(
            unit_record(&bytes[..n], 0, 87, "fixture").is_err(),
            "prefix {n}"
        );
    }
    assert!(unit_record(&bytes, 0, 88, "fixture").is_err());
    assert!(unit_record(&bytes, 0, 87, "other").is_err());
    let mut bad = bytes;
    bad[24..28].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(unit_record(&bad, 0, 87, "fixture").is_err());
}

#[test]
fn de_numeric_survey_rejects_unknown_layouts_and_missing_records() {
    assert!(survey(b"VER 8.8\0").is_err());
    assert!(survey(b"VER 8.9\0").is_err());
}

#[test]
fn de_record_reads_fixed_stats_and_bounds_every_variable_count() {
    let mut bytes = vec![0; 136 + 21];
    bytes[0] = 80;
    bytes[1..3].copy_from_slice(&87_i16.to_le_bytes());
    bytes[22..24].copy_from_slice(&1234_i16.to_le_bytes());
    bytes[24..28].copy_from_slice(&6_f32.to_le_bytes());
    bytes[29..33].copy_from_slice(&1.5_f32.to_le_bytes());
    bytes[33..37].copy_from_slice(&1.5_f32.to_le_bytes());
    bytes.push(255); // damage graphic count is bounded before a skip
    assert!(unit_record(&bytes, 0, 87, "fixture").is_err());
    bytes[157] = 0;
    bytes.extend_from_slice(&[0; 22]);
    bytes.extend_from_slice(&[0x60, 0x0a, 7, 0]);
    bytes.extend_from_slice(b"fixture");
    bytes.extend_from_slice(&87_i16.to_le_bytes());
    bytes.extend_from_slice(&87_i16.to_le_bytes());
    bytes.extend_from_slice(&[0; 4 + 41 + 10]);
    bytes.extend_from_slice(&1000_u16.to_le_bytes()); // drop sites
    assert!(unit_record(&bytes, 0, 87, "fixture").is_err());
}
