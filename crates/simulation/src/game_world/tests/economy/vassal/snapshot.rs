//! A saved Vassal is back in its Castle with its development switch.
use super::*;

#[test]
fn a_vassal_stays_in_its_castle_with_development_off_through_a_save() {
    let (mut w, v, castle) = setup();
    w.issue_vassal_development(PlayerId(0), v, false).unwrap();
    assert!(!w.vassal_status(v).unwrap().development);
    let (restored, report) = crate::game_world::tests::snapshot_roundtrip::roundtrip(&w, world());
    assert_eq!(
        (report.units_lost, report.buildings_lost),
        (0, 0),
        "{report:?}"
    );
    let garrison = &restored.building(castle).unwrap().garrison;
    assert!(
        garrison.iter().any(|held| held.unit.id == v),
        "sheltered again"
    );
    let status = restored.vassal_status(v).expect("the domain is active");
    assert!(!status.development, "development stays off");
}
