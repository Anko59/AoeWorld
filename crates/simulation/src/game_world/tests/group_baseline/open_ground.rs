//! S1-S3: a long Line order on open ground, and through a 2-tile forest pass.
use super::*;

const START: (i32, i32) = (40, 40);

fn line_order(w: &mut GameWorld, ids: &[EntityId], to: WorldPosition) {
    w.issue_formation_move(PlayerId(0), ids, Formation::Line, to, false, false)
        .unwrap();
}

#[test]
fn sixty_militia_in_line_cross_open_ground() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = block(&mut w, PlayerId(0), START, 10, 60);
    let to = center(140, 43);
    line_order(&mut w, &ids, to);
    let m = march(&mut w, &ids, to, 4000);
    note("line60-open", &m.fields());
    // Measured: the first unit stops at tick 2159 and the last at 2328, so the
    // tail arrives 169 ticks (8.5 s) after the head.
    // PR 6 flips this: the last arrival is within 60 ticks (3 s) of the first.
    assert_eq!((m.first(), m.last()), (2159, 2328));
    assert_eq!(m.stopped(), 60);
    assert_eq!(m.stopped_short, 0);
    // No single-file column forms on open ground, and the block never
    // stretches beyond ten tiles. The column appears only at an obstacle.
    assert_eq!((m.column_ticks, m.longest_tiles), (0, 10));
}

#[test]
fn sixty_militia_in_line_cross_a_two_tile_forest_pass() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    // Six tiles of trees across x 70..76, open only at y 42 and 43.
    forest(&mut w, TileRect::from_xywh(70, 20, 6, 22));
    forest(&mut w, TileRect::from_xywh(70, 44, 6, 22));
    let ids = block(&mut w, PlayerId(0), START, 10, 60);
    let to = center(140, 43);
    line_order(&mut w, &ids, to);
    let m = march(&mut w, &ids, to, 6000);
    note("line60-pass", &m.fields());
    // Measured: the last unit stops at tick 3955, 1.70 times the open-ground
    // 2328. PR 6 flips this to at most 1.6 times.
    assert_eq!((m.first(), m.last()), (2205, 3955));
    assert_eq!(m.last() * 100 / 2328, 169);
    assert_eq!(m.stopped(), 60);
    // S3: 14 units stop more than eight tiles from the destination while the
    // head has arrived. PR 3 flips this to 0: every unit walks to its own slot.
    assert_eq!(m.stopped_short, 14);
    // S1: the group is single file (never more than one tile across) for 1,828
    // ticks and stretches 78 tiles. PR 6 deletes the column.
    assert_eq!(
        (m.column_ticks, m.longest_tiles, m.min_frontage_when_long),
        (1828, 78, 0)
    );
}
