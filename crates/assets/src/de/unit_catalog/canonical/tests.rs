use super::*;
use crate::dat::de_survey::graphics::{Animation, Part};

fn selected(sprite: u32) -> Selected {
    Selected {
        unit: 539,
        icon: 42,
        pose: 121,
        sprite,
        file: format!("dat_{sprite}"),
        frames: 2,
        angles: 1,
        cells: 2,
        seconds: 0.1,
        replay: 0.0,
        terminal: false,
        parts: vec![Part {
            graphic: sprite as i16,
            x: 0,
            y: 0,
            layer: 20,
            animation: Animation {
                file: "native_ship_x1".into(),
                sprite: sprite as i32,
                frames: 2,
                angles: 1,
                seconds: 0.1,
                replay_delay: 0.0,
            },
        }],
    }
}

#[test]
fn exact_native_recipes_share_storage_without_changing_bindings_or_cells() {
    let mut alias = selected(20);
    alias.unit = 545;
    alias.pose = 125;
    alias.terminal = true;
    let mut records = vec![selected(10), alias];
    apply(&mut records).unwrap();
    assert_eq!(
        (records[1].sprite, records[1].file.as_str()),
        (10, "dat_10")
    );
    assert_eq!(
        (records[1].unit, records[1].pose, records[1].cells),
        (545, 125, 2)
    );
    assert_eq!(records[1].parts[0].graphic, 20);
    assert!(records[1].terminal);
}

#[test]
fn offsets_order_sources_and_native_timing_keep_distinct_storage() {
    let original = selected(10);
    let mut cases = Vec::new();
    let mut changed = selected(20);
    changed.parts[0].x = 1;
    cases.push(changed);
    let mut changed = selected(30);
    changed.parts[0].animation.file = "other_x1".into();
    cases.push(changed);
    let mut changed = selected(40);
    changed.seconds = 0.2;
    cases.push(changed);
    let mut changed = selected(50);
    changed.parts[0].animation.replay_delay = 1.0;
    cases.push(changed);
    let mut changed = selected(60);
    changed.parts.push(changed.parts[0].clone());
    cases.push(changed);
    for changed in cases {
        let expected = changed.sprite;
        let mut records = vec![original.clone(), changed];
        apply(&mut records).unwrap();
        assert_eq!(records[1].sprite, expected);
    }
}
