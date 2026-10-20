use super::*;

#[test]
fn corpse_art_follows_the_unit_dead_reference_instead_of_an_adjacent_slp_guess() {
    let mut file = parse(&fixture_with(0, 83)).unwrap();
    file.units
        .iter_mut()
        .find(|u| u.id == 83)
        .unwrap()
        .dead_unit = 0;
    let table = table::build(&file, None);
    let row = table.rows.iter().find(|r| r.unit == 83).unwrap();
    assert!(
        row.graphics
            .iter()
            .any(|g| g.role == "decay" && g.slp == 2223)
    );
}
