//! A saved General keeps its settings and takes its army again.
use super::*;

#[test]
fn a_general_keeps_training_mix_and_army_through_a_save() {
    let (mut w, _castle) = setup();
    for i in 0..12 {
        soldier(&mut w, UnitKind::Militia, 100 + i % 4, 100 + i / 4);
    }
    let g = soldier(&mut w, UnitKind::General, 104, 104);
    decide(&mut w);
    let mix = [10, 20, 30, 40, 0];
    w.issue_general_training(PlayerId(0), g, true).unwrap();
    w.issue_general_mix(PlayerId(0), g, mix).unwrap();
    let before = w.general_status(g).unwrap();
    assert!(before.army_size > 0, "the General has an army to keep");
    let (mut restored, report) =
        crate::game_world::tests::snapshot_roundtrip::roundtrip(&w, world());
    assert_eq!(
        (report.units_lost, report.buildings_lost),
        (0, 0),
        "{report:?}"
    );
    assert!(report.managers >= 1, "{report:?}");
    let army = &restored.economy.generals.armies[&g];
    assert_eq!((army.training, army.mix), (true, mix));
    decide(&mut restored);
    let after = restored.general_status(g).unwrap();
    assert_eq!(after.army_size, before.army_size, "the army is taken again");
}
