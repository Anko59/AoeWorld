//! Synthetic DAT graphs: no licensed pixels or files are needed.
use super::*;
use de_survey::graphics::Animation;

fn graphic(id: i16, file: &str, layer: u8) -> (i16, Graphic) {
    let animation = Animation {
        file: file.into(),
        sprite: -1,
        frames: 1,
        angles: 1,
        seconds: 0.0,
        replay_delay: 0.0,
    };
    let part = Part {
        animation,
        graphic: id,
        x: 0,
        y: 0,
        layer,
    };
    (id, Graphic { parts: vec![part] })
}

fn unit(id: i16, standing: i16, annexes: &[(i16, [f32; 2])]) -> (i16, de_units::Unit) {
    let building = de_units::Building {
        construction: -1,
        snow: -1,
        destruction: -1,
        rubble: -1,
        researching: -1,
        research_complete: -1,
        foundation: 27,
        annexes: annexes
            .iter()
            .map(|&(unit, offset)| de_units::Annex { unit, offset })
            .collect(),
    };
    let unit = de_units::Unit {
        id,
        name: String::new(),
        standing,
        dying: -1,
        dead: -1,
        walking: -1,
        running: -1,
        attack: -1,
        work: -1,
        carrying: -1,
        release: -1,
        footprint: [4.0; 2],
        damage: Vec::new(),
        building: Some(building),
    };
    (id, unit)
}

fn assemble(
    graphics: Vec<(i16, Graphic)>,
    units: Vec<(i16, de_units::Unit)>,
    head: &str,
) -> Vec<(String, [f32; 2])> {
    let graphics: BTreeMap<_, _> = graphics.into_iter().collect();
    let units: BTreeMap<_, _> = units.into_iter().collect();
    let mut resolver = Resolver {
        graphics: &graphics,
        units: &units,
        directory: PathBuf::from("absent"),
        disabled: BTreeSet::new(),
        sources: BTreeSet::new(),
    };
    resolver
        .assembled(1, Some(head))
        .unwrap()
        .into_iter()
        .map(|p| (p.source, p.misplacement))
        .collect()
}

#[test]
fn age_family_slices_share_the_origin_and_keep_back_to_front_order() {
    // Town Center: the annex offsets order main, back, centre and front, but
    // every DE slice is already authored around the building origin.
    let graphics = vec![
        graphic(10, "b_dark_town_center_age1_back_x1", 20),
        graphic(11, "b_dark_town_center_age1_main_x1", 20),
        graphic(12, "b_dark_town_center_age1_center_x1", 20),
        graphic(13, "b_dark_town_center_age1_front_x1", 20),
        // Shared Dark Age annex graphics are replaced by the head's age family.
        graphic(21, "b_dark_town_center_age1_main_x1", 20),
        graphic(22, "b_dark_town_center_age1_center_x1", 20),
        graphic(23, "b_dark_town_center_age1_front_x1", 20),
    ];
    let units = vec![
        unit(
            1,
            10,
            &[(2, [1.0, -1.0]), (3, [-0.5, 0.5]), (4, [-1.0, 1.0])],
        ),
        unit(2, 21, &[]),
        unit(3, 22, &[]),
        unit(4, 23, &[]),
    ];
    let parts = assemble(graphics, units, "b_dark_town_center_age1_back_x1");
    let names: Vec<_> = parts.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(
        names,
        [
            "de/b_dark_town_center_age1_main_x1.sld",
            "de/b_dark_town_center_age1_back_x1.sld",
            "de/b_dark_town_center_age1_center_x1.sld",
            "de/b_dark_town_center_age1_front_x1.sld",
        ]
    );
    assert!(parts.iter().all(|p| p.1 == [0.0; 2]), "{parts:?}");
}

#[test]
fn separate_annex_sprites_are_placed_by_their_offsets() {
    // Gate corner towers do not belong to the head's age family: the offset
    // moves their pixels, and the nearer tower draws after the farther one.
    let graphics = vec![
        graphic(10, "b_west_gate_stone_ne_closed_x1", 20),
        graphic(11, "b_west_gate_stone_corner_x1", 20),
    ];
    let units = vec![
        unit(1, 10, &[(2, [1.5, 0.0]), (3, [-1.5, 0.0])]),
        unit(2, 11, &[]),
        unit(3, 11, &[]),
    ];
    let parts = assemble(graphics, units, "b_west_gate_stone_ne_closed_x1");
    let offsets: Vec<_> = parts.iter().map(|p| p.1).collect();
    assert_eq!(offsets, [[1.5, 0.0], [0.0, 0.0], [-1.5, 0.0]]);
}
