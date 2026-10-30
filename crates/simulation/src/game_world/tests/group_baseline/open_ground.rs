//! S1-S3: block movement on open ground and through a two-tile forest pass.
use super::*;

const START: (i32, i32) = (40, 40);

fn line_march(pass: bool) -> March {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    if pass {
        forest(&mut w, TileRect::from_xywh(70, 20, 6, 22));
        forest(&mut w, TileRect::from_xywh(70, 44, 6, 22));
    }
    let ids = block(&mut w, PlayerId(0), START, 10, 60);
    let to = center(140, 43);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, false)
        .unwrap();
    let m = march(&mut w, &ids, to, if pass { 6000 } else { 4000 });
    note(
        if pass { "line60-pass" } else { "line60-open" },
        &m.fields(),
    );
    for &id in &ids {
        assert_eq!(
            w.unit(id).unwrap().position,
            w.formation_destination_slot(id).unwrap(),
            "pass={pass} id={id:?}: an arrival is its assigned slot, not the give-up timer"
        );
    }
    m
}

#[test]
fn sixty_militia_in_line_cross_open_ground() {
    let m = line_march(false);
    assert_eq!(m.stopped(), 60);
    assert_eq!(m.stopped_short, 0);
    assert!(
        m.last() - m.first() <= 60,
        "last arrival must follow the first within three seconds: {m:?}"
    );
    assert_eq!((m.column_ticks, m.longest_tiles), (0, 8));
    assert_eq!(m.min_frontage_when_long, i64::MAX);
    // Rear-only open-ground cohesion advances first/last arrival; the exact
    // slots, shape and three-second settling bound above stay unchanged.
    assert_eq!((m.first(), m.last()), (2165, 2209));
}

#[test]
fn sixty_militia_in_line_cross_a_two_tile_forest_pass() {
    let open = line_march(false);
    let pass = line_march(true);
    assert_eq!(pass.stopped(), 60);
    assert_eq!(pass.stopped_short, 0);
    assert!(
        pass.last() * 10 <= open.last() * 16,
        "pass/open must be at most 1.6: {pass:?} / {open:?}"
    );
    assert_eq!((pass.column_ticks, pass.longest_tiles), (0, 16));
    assert_eq!(pass.min_frontage_when_long, i64::MAX);
    assert_eq!((pass.first(), pass.last()), (2370, 2515));
}
