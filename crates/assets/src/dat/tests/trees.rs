use super::*;

#[test]
fn tree_table_includes_species_chopped_graphics_and_the_decorative_stump() {
    let Ok(bytes) = std::fs::read("../../local-assets/trial/Data/empires2.dat") else {
        return;
    };
    let file = decode(&bytes).unwrap();
    let table = table::build(&file, None);
    for (unit, full, chopped) in [(349, 4652, 2299), (350, 4654, 2307), (351, 4653, 2303)] {
        let row = table.rows.iter().find(|row| row.unit == unit).unwrap();
        assert!(
            row.graphics
                .iter()
                .any(|g| g.role == "stand" && g.slp == full)
        );
        assert!(
            row.family
                .iter()
                .any(|g| g.slp == chopped && g.frames == 1 && g.angles == 1)
        );
        assert_eq!(row.gameplay.as_ref().unwrap().dead_unit, 415);
        let stage = crate::catalog::trees::STAGES
            .iter()
            .find(|stage| stage.unit == unit)
            .unwrap();
        assert!(row.graphics.iter().any(|g| g.role == "dying"
            && g.graphic == stage.chopped_graphic
            && g.slp == stage.chopped_slp as i32));
    }
    let stump = table.rows.iter().find(|row| row.unit == 415).unwrap();
    assert!(
        stump
            .graphics
            .iter()
            .any(|g| g.graphic == 1698 && g.slp == 1035)
    );
}
