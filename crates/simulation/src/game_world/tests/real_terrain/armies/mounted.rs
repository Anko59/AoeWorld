//! Cavalry flanks stand 20 tiles beside a General's goal. Beside a river that
//! ground lies across the water: the flank must still march, and the General
//! must not retry a refused order every decision.
use super::march::run_cell;
use super::scene::{Cell, Shape};
use super::*;

#[test]
fn cavalry_flanks_beside_a_river_march_with_their_general() {
    let row = run_cell(
        Cell {
            shape: Shape::Bank,
            kind: UnitKind::Paladin,
            count: 100,
            general: true,
        },
        120,
    );
    assert_eq!(row.arrived, 100, "{}\n{}", row.tsv(), row.diagnosis);
    assert!(row.orders <= 30, "{}", row.tsv());
}
