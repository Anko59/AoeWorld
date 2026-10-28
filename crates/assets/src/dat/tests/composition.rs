use super::*;

#[test]
fn delta_records_preserve_signed_offsets_angle_and_order() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes());
    graphic(&mut bytes, 0, "SYNTH", 10, 1, 1, &[7, 9]);
    for (at, x, y, angle) in [(84 + 8, -37i16, 19i16, -1i16), (100 + 8, 12, -23, 4)] {
        bytes[at..at + 2].copy_from_slice(&x.to_le_bytes());
        bytes[at + 2..at + 4].copy_from_slice(&y.to_le_bytes());
        bytes[at + 4..at + 6].copy_from_slice(&angle.to_le_bytes());
    }
    let graphics = sections::graphics(&mut reader::Reader::new(&bytes, 0)).unwrap();
    let deltas = &graphics[0].deltas;
    assert_eq!(
        deltas
            .iter()
            .map(|d| (d.graphic_id, d.offset_x, d.offset_y, d.display_angle))
            .collect::<Vec<_>>(),
        [(7, -37, 19, -1), (9, 12, -23, 4)]
    );
}

#[test]
fn terrain_records_resolve_foundation_art_without_a_guessed_terrain_id() {
    let mut bytes = fixture(0);
    let at = bytes.windows(10).position(|w| w == b"Synthetic\0").unwrap() - 2;
    // A foundation terrain redirects to terrain 3, whose SLP is 15003.
    bytes[at + 27 * 416 + 192..at + 27 * 416 + 194].copy_from_slice(&3i16.to_le_bytes());
    let file = parse(&bytes).unwrap();
    assert_eq!(file.terrains.len(), 32);
    assert_eq!(file.terrain_slp(27), Some(15003));
    assert_eq!(file.terrain_texture(27).unwrap().dimensions, [10, 10]);
}

#[test]
fn aok_building_section_has_no_snow_graphic_and_keeps_annex_tiles() {
    let mut bytes = fixture(0);
    let name = bytes.windows(5).position(|w| w == b"HOUS\0").unwrap();
    let unit_start = name - (146 + 2 * 5 + 6);
    bytes[unit_start + 27..unit_start + 31].copy_from_slice(&1.5f32.to_le_bytes());
    bytes[unit_start + 31..unit_start + 35].copy_from_slice(&2.0f32.to_le_bytes());
    bytes[unit_start + 43..unit_start + 45].copy_from_slice(&2i16.to_le_bytes());
    // The first unit ends immediately before the villager. Its last 80 bytes
    // are a deliberately non-symmetric synthetic AoK building record.
    let start = name + 5 + 4 + 4 + 37 + 20 + 1 + 6 + 6 + 56 + 65;
    let b = &mut bytes[start..start + 80];
    b[2] = 3;
    b[3..5].copy_from_slice(&123i16.to_le_bytes());
    b[8..10].copy_from_slice(&27i16.to_le_bytes());
    for (slot, (id, x, y)) in [
        (2i16, -1.5f32, 0.5f32),
        (-1, 0.0, 0.0),
        (2, 1.0, -2.0),
        (-1, 0.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        let at = 15 + slot * 10;
        b[at..at + 2].copy_from_slice(&id.to_le_bytes());
        b[at + 2..at + 6].copy_from_slice(&x.to_le_bytes());
        b[at + 6..at + 10].copy_from_slice(&y.to_le_bytes());
    }
    b[55..57].copy_from_slice(&42i16.to_le_bytes());
    let file = parse(&bytes).unwrap();
    assert_eq!(file.report.stopped, None);
    let unit = file.unit(0).unwrap();
    assert_eq!(unit.footprint, [3.0, 4.0]);
    assert_eq!(unit.dead_unit, 2);
    assert_eq!(unit.dying, 7);
    assert_eq!(unit.attack_graphic, Some(-1));
    let building = unit.building.as_ref().unwrap();
    assert_eq!(
        (
            building.adjacent_mode,
            building.graphics_angle,
            building.foundation_terrain,
            building.head_unit
        ),
        (3, 123, 27, 42)
    );
    assert_eq!(
        (
            building.annexes[0].unit_id,
            building.annexes[0].misplacement
        ),
        (2, [-1.5, 0.5])
    );
    assert_eq!(building.annexes[2].misplacement, [1.0, -2.0]);
    let table = table::build(&file, None);
    assert!(
        serde_json::to_string(&table)
            .unwrap()
            .contains("foundation_terrain")
    );
}

#[test]
fn composition_sorts_annexes_by_depth_but_keeps_each_delta_order() {
    let mut file = parse(&fixture(0)).unwrap();
    let annex = Annex {
        unit_id: 2,
        misplacement: [1.0, -0.5],
    };
    file.units[0].building.as_mut().unwrap().annexes = [
        annex,
        Annex {
            unit_id: -1,
            misplacement: [0.0; 2],
        },
        Annex {
            unit_id: -1,
            misplacement: [0.0; 2],
        },
        Annex {
            unit_id: -1,
            misplacement: [0.0; 2],
        },
    ];
    file.units[0].standing[0] = 3;
    let parts = file.building_composition(0).unwrap().parts;
    assert_eq!(
        parts.iter().map(|p| p.graphic_id).collect::<Vec<_>>(),
        [4, 3, 2]
    );
    assert_eq!(parts[0].misplacement, [1.0, -0.5]);
    assert_eq!(parts[1].misplacement, [0.0; 2]);
}

#[test]
fn real_trial_composition_matches_raw_records_and_resolves_tc_annex_graphics() {
    let Ok(path) = std::env::var("AOE_TRIAL_DAT") else {
        return;
    };
    let compressed = std::fs::read(&path).expect("AOE_TRIAL_DAT");
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::DeflateDecoder::new(&compressed[..]),
        &mut bytes,
    )
    .unwrap();
    let file = parse(&bytes).unwrap();
    assert_eq!(file.report.stopped, None);
    for id in [109, 618, 70, 68, 562, 584, 12, 598] {
        let unit = file.unit(id).unwrap();
        let b = unit.building.as_ref().unwrap();
        let raw = &bytes[b.byte_offset..b.byte_offset + 80];
        assert_eq!(
            b.foundation_terrain,
            i16::from_le_bytes(raw[8..10].try_into().unwrap())
        );
        for (slot, annex) in b.annexes.iter().enumerate() {
            let raw = &raw[15 + slot * 10..25 + slot * 10];
            assert_eq!(
                annex.unit_id,
                i16::from_le_bytes(raw[..2].try_into().unwrap())
            );
            assert_eq!(
                annex.misplacement,
                [
                    f32::from_le_bytes(raw[2..6].try_into().unwrap()),
                    f32::from_le_bytes(raw[6..10].try_into().unwrap())
                ]
            );
        }
        eprintln!(
            "unit {id} {} stand={} footprint={:?} building={b:?}",
            unit.name, unit.standing[0], unit.footprint
        );
    }
    let head = file.unit(109).unwrap().building.as_ref().unwrap();
    assert_eq!(head.foundation_terrain, 27);
    assert_eq!(
        head.annexes.map(|a| (a.unit_id, a.misplacement)),
        [
            (618, [1.0, -1.0]),
            (619, [-0.5, 0.5]),
            (620, [-1.0, 1.0]),
            (-1, [0.0, 0.0])
        ]
    );
    assert!(head.annexes.iter().filter(|a| a.unit_id >= 0).count() >= 3);
    for annex in head.annexes.iter().filter(|a| a.unit_id >= 0) {
        let annex = file.unit(annex.unit_id.into()).unwrap();
        assert!(
            file.graphic(annex.standing[0].into())
                .unwrap()
                .name
                .starts_with("RTWC")
        );
    }
    for id in [433, 434, 435, 3239, 3240, 3241, 5468, 5469, 5470] {
        let g = file.graphic(id).unwrap();
        let raw = &bytes[g.byte_offset..];
        for (slot, d) in g.deltas.iter().enumerate() {
            let raw = &raw[78 + slot * 16..94 + slot * 16];
            assert_eq!(
                (d.offset_x, d.offset_y),
                (
                    i16::from_le_bytes(raw[8..10].try_into().unwrap()),
                    i16::from_le_bytes(raw[10..12].try_into().unwrap())
                )
            );
        }
        eprintln!("graphic {id}: {g:?}");
    }
    let composition = file.building_composition(109).unwrap();
    // Every wing attachment has its own single-frame, single-angle SLP.
    // In particular, 4610 belongs to annex 620, not to the head or annex 619.
    for (unit, stand, body, attachment, displacement) in [
        (109, 3241, 3596, 4612, [0.0, 0.0]),
        (619, 3240, 3595, 4611, [-0.5, 0.5]),
        (620, 3239, 3594, 4610, [-1.0, 1.0]),
    ] {
        assert_eq!(file.unit(unit).unwrap().standing[0], stand);
        let selected = composition
            .parts
            .iter()
            .filter(|p| p.misplacement == displacement && p.layer == 20)
            .map(|p| (p.slp_id, p.frames, p.angles))
            .collect::<Vec<_>>();
        assert_eq!(selected, [(body, 1, 1), (attachment, 1, 1)]);
    }
    assert_eq!(composition.foundation_slp, Some(15006));
    assert_eq!(
        composition
            .parts
            .iter()
            .map(|p| p.graphic_id)
            .collect::<Vec<_>>(),
        [433, 434, 435, 3241, 5470, 3240, 5469, 3239, 5468]
    );
    assert!(composition.parts.iter().all(|p| p.offset == [0, 0]));
    assert!(composition.parts.iter().any(|p| p.slp_id == 891));
    let archive =
        std::fs::read(std::path::Path::new(&path).with_file_name("graphics.drs")).unwrap();
    let pack = table::slp_ids(&archive).unwrap();
    let entries = crate::drs::parse(&archive).unwrap();
    for id in [3594, 3595, 3596, 4610, 4611, 4612] {
        let entry = entries
            .iter()
            .find(|e| e.id == id && e.kind == *b" pls")
            .unwrap();
        let frames = crate::slp::parse(entry.data).unwrap();
        assert_eq!(frames.len(), 1, "SLP {id} has no alternate facing");
        let f = &frames[0];
        eprintln!(
            "SLP {id}: {}x{}, anchor ({},{})",
            f.width, f.height, f.hotspot_x, f.hotspot_y
        );
    }
    assert!(!pack.contains(&890));
    assert_eq!(
        composition
            .parts
            .iter()
            .filter(|p| pack.contains(&p.slp_id))
            .count(),
        8
    );
    eprintln!("TC composition: {composition:?}");
}

#[test]
fn cyclic_graphic_deltas_and_missing_annexes_fail_explicitly() {
    let mut file = parse(&fixture(0)).unwrap();
    file.units[0].standing[0] = 3;
    file.graphics.iter_mut().find(|g| g.id == 3).unwrap().deltas[0].graphic_id = 3;
    assert!(
        file.building_composition(0)
            .unwrap_err()
            .to_string()
            .contains("cyclic")
    );
    file.units[0].standing[0] = 2;
    file.units[0].building.as_mut().unwrap().annexes[0].unit_id = 999;
    assert!(
        file.building_composition(0)
            .unwrap_err()
            .to_string()
            .contains("missing annex")
    );
}

#[test]
fn annex_age_family_is_selected_before_its_own_delta_offsets() {
    let mut file = parse(&fixture(0)).unwrap();
    file.units[0].standing[0] = 3;
    file.units[0].building.as_mut().unwrap().annexes[0] = Annex {
        unit_id: 2,
        misplacement: [-1.0, 0.5],
    };
    file.graphics.iter_mut().find(|g| g.id == 3).unwrap().name = "RTWC2N4W".into();
    file.graphics.iter_mut().find(|g| g.id == 4).unwrap().name = "RTWC1N3G".into();
    let mut alternate = file.graphic(4).unwrap().clone();
    alternate.id = 100;
    alternate.name = "RTWC2N3W".into();
    alternate.slp_id = 90000;
    alternate.deltas = vec![GraphicDelta {
        graphic_id: 2,
        offset_x: -17,
        offset_y: 29,
        display_angle: 0,
    }];
    file.graphic_index.insert(100, file.graphics.len());
    file.graphics.push(alternate);
    let composition = file.building_composition(0).unwrap();
    assert!(!composition.parts.iter().any(|p| p.graphic_id == 4));
    let annex = composition
        .parts
        .iter()
        .find(|p| p.graphic_id == 100)
        .unwrap();
    assert_eq!(annex.slp_id, 90000);
    assert_eq!(annex.misplacement, [-1.0, 0.5]);
    assert!(
        composition
            .parts
            .iter()
            .any(|p| p.graphic_id == 2 && p.offset == [-17, 29] && p.misplacement == [-1.0, 0.5])
    );
}

#[test]
fn real_trial_castle_university_composes_each_native_layer_once() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        return;
    };
    let file = decode(&bytes).unwrap();
    let recipe = file.building_composition(209).unwrap();
    for slp in [1359, 1363, 3835] {
        assert_eq!(recipe.parts.iter().filter(|p| p.slp_id == slp).count(), 1);
    }
}

#[test]
fn measured_castle_composite_back_references_emit_each_layer_once() {
    for (animation, body) in [(569, 573), (65, 69)] {
        let mut file = parse(&fixture(0)).unwrap();
        let mut a = file.graphics[0].clone();
        a.id = animation;
        a.slp_id = 1001;
        a.deltas = vec![GraphicDelta {
            graphic_id: body as i16,
            offset_x: 0,
            offset_y: 0,
            display_angle: -1,
        }];
        let mut b = a.clone();
        b.id = body;
        b.slp_id = 1002;
        b.deltas[0].graphic_id = animation as i16;
        file.graphic_index.insert(animation, file.graphics.len());
        file.graphic_index.insert(body, file.graphics.len() + 1);
        file.graphics.extend([a, b]);
        file.units[0].standing[0] = animation as i16;
        let composed = file.building_composition(0).unwrap();
        for slp in [1001, 1002] {
            assert_eq!(composed.parts.iter().filter(|p| p.slp_id == slp).count(), 1);
        }
    }
}

#[test]
fn unit_damage_graphics_preserve_threshold_and_apply_mode() {
    let mut bytes = fixture(0);
    let name = bytes.windows(5).position(|w| w == b"HOUS\0").unwrap();
    let start = name - (146 + 2 * 5 + 6);
    bytes[start + 146..start + 151].copy_from_slice(&[3, 0, 50, 0, 1]);
    bytes[start + 151..start + 156].copy_from_slice(&[4, 0, 75, 0, 2]);
    let file = parse(&bytes).unwrap();
    let damage = &file.unit(0).unwrap().damage_graphics;
    assert_eq!(
        damage
            .iter()
            .map(|d| (d.graphic_id, d.damage_percent, d.apply_mode))
            .collect::<Vec<_>>(),
        [(3, 50, 1), (4, 75, 2)]
    );
}

#[test]
fn building_composition_retains_damage_graphic_stages() {
    let mut bytes = fixture(0);
    let name = bytes.windows(5).position(|w| w == b"HOUS\0").unwrap();
    let start = name - (146 + 2 * 5 + 6);
    bytes[start + 146..start + 151].copy_from_slice(&[3, 0, 25, 0, 0]);
    bytes[start + 151..start + 156].copy_from_slice(&[4, 0, 75, 0, 0]);
    let file = parse(&bytes).unwrap();
    let stages = file.building_composition(0).unwrap().damage;
    assert_eq!(
        stages
            .iter()
            .map(|stage| (stage.damage_percent, stage.apply_mode))
            .collect::<Vec<_>>(),
        [(25, 0), (75, 0)]
    );
    assert!(stages.iter().all(|stage| !stage.parts.is_empty()));
}
