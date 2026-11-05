//! What a blocked part of a formation does: it passes friends on one-lane
//! ground, it asks for a new route within a bounded time, and it never waits
//! for ever beside a slot another army took.
use super::march::run_cell;
use super::scene::{Cell, Shape};
use super::*;

fn march(shape: Shape, kind: UnitKind, count: usize, general: bool) -> super::march::Row {
    run_cell(
        Cell {
            shape,
            kind,
            count,
            general,
        },
        200,
    )
}

/// Two Generals sent to one point give their squads the same slots.
#[test]
fn two_armies_sharing_a_destination_both_finish_arriving() {
    let row = march(Shape::Forest, UnitKind::Champion, 400, true);
    assert_eq!(row.stuck, 0, "{}\n{}", row.tsv(), row.diagnosis);
    assert_eq!(row.arrived, 400, "{}\n{}", row.tsv(), row.diagnosis);
}

#[test]
fn rams_file_through_a_one_tile_pass_without_a_knot() {
    let row = march(Shape::Pass, UnitKind::SiegeRam, 100, false);
    assert_eq!(row.arrived, 100, "{}\n{}", row.tsv(), row.diagnosis);
}

/// A body of a full tile used to wedge at every gap its route went through.
#[test]
fn mangonels_fit_any_gap_their_route_takes() {
    let row = march(Shape::Pass, UnitKind::Mangonel, 20, false);
    assert_eq!(row.arrived, 20, "{}\n{}", row.tsv(), row.diagnosis);
}
