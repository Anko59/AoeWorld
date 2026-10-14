use super::{
    mark, parse,
    pixels::{paint, solid},
};
use crate::ship::compare::{
    Shot, compare_view,
    pixels::{Image, Rect},
    report::{self, Camera, Capture, Status},
    settle::Sample,
};

fn camera(x: f64, zoom: f64) -> Camera {
    Camera { x, y: 200.0, zoom }
}

fn shot(image: &Image, at: Camera) -> Shot {
    Shot {
        png: Some(image.encode().expect("encode")),
        capture: Capture {
            camera: Some(at),
            history: vec![
                Sample {
                    at: 0,
                    pending: 0,
                    failed: 0,
                    frame: 1,
                },
                Sample {
                    at: 1500,
                    pending: 0,
                    failed: 0,
                    frame: 3,
                },
            ],
            settle_ms: Some(2400),
            scene: Some(Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            }),
            masks: vec![Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 2,
            }],
            ..Capture::default()
        },
    }
}

fn bookmark() -> crate::ship::compare::bookmarks::Bookmark {
    parse(&[mark("alps")]).expect("file").bookmarks.remove(0)
}

#[test]
fn a_view_is_identical_changed_mismatched_or_failed() {
    let out = tempfile::tempdir().expect("dir");
    let before = solid(20, 10, [5, 5, 5, 255]);
    let mut after = before.clone();
    let same = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(10.5, 1.0))),
        Some(&shot(&after, camera(10.5, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    assert_eq!(
        (same.status, same.changed_pixels),
        (Status::Identical, Some(0))
    );
    assert!(
        out.path().join("alps.diff.png").is_file() && out.path().join("alps.montage.png").is_file()
    );

    paint(
        &mut after,
        Rect {
            x: 8,
            y: 4,
            w: 3,
            h: 2,
        },
        [50, 5, 5, 255],
    );
    paint(
        &mut after,
        Rect {
            x: 1,
            y: 1,
            w: 1,
            h: 1,
        },
        [50, 5, 5, 255],
    ); // under the mask
    let changed = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(10.5, 1.0))),
        Some(&shot(&after, camera(10.5, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    assert_eq!(
        (
            changed.status,
            changed.changed_pixels,
            changed.masked_changed_pixels
        ),
        (Status::Changed, Some(6), Some(1))
    );
    assert!(!changed.only_masked_differ);

    let mismatch = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(10.5, 1.0))),
        Some(&shot(&after, camera(30.5, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    assert_eq!(
        (mismatch.status, mismatch.changed_pixels),
        (Status::CameraMismatch, None)
    );
    assert!(mismatch.error.expect("reason").contains("30.50"));

    let broken = Shot {
        png: None,
        capture: Capture {
            error: Some("not settled".into()),
            ..Capture::default()
        },
    };
    let failed = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(10.5, 1.0))),
        Some(&broken),
        out.path(),
    )
    .expect("view")
    .0;
    assert_eq!(failed.status, Status::Failed);
    assert!(failed.error.expect("reason").contains("after: not settled"));
    assert_eq!(
        compare_view(&bookmark(), None, None, out.path())
            .expect("view")
            .0
            .status,
        Status::Failed
    );
}

#[test]
fn a_difference_only_under_the_hud_is_flagged_not_counted() {
    let out = tempfile::tempdir().expect("dir");
    let before = solid(20, 10, [5, 5, 5, 255]);
    let mut after = before.clone();
    paint(
        &mut after,
        Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        },
        [90, 5, 5, 255],
    );
    let view = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(1.0, 1.0))),
        Some(&shot(&after, camera(1.0, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    assert_eq!(
        (view.status, view.changed_pixels, view.masked_changed_pixels),
        (Status::Identical, Some(0), Some(4))
    );
    assert!(view.only_masked_differ);
    assert!(report::table(&[view]).contains("HUD only"));
}

#[test]
fn the_table_has_one_row_per_view() {
    let out = tempfile::tempdir().expect("dir");
    let before = solid(20, 10, [5, 5, 5, 255]);
    let mut after = before.clone();
    paint(
        &mut after,
        Rect {
            x: 8,
            y: 4,
            w: 3,
            h: 2,
        },
        [50, 5, 5, 255],
    );
    let identical = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(1.0, 1.0))),
        Some(&shot(&before, camera(1.0, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    let changed = compare_view(
        &bookmark(),
        Some(&shot(&before, camera(1.0, 1.0))),
        Some(&shot(&after, camera(1.0, 1.0))),
        out.path(),
    )
    .expect("view")
    .0;
    let table = report::table(&[identical, changed]);
    let rows: Vec<&str> = table.lines().collect();
    assert_eq!(rows.len(), 3);
    assert!(
        rows[0].starts_with("view") && rows[1].contains("identical") && rows[2].contains("6 px")
    );
    assert!(
        rows[2].contains("3.1250%"),
        "6 of the 192 unmasked pixels: {}",
        rows[2]
    );
}

#[test]
fn cameras_are_the_same_view_within_a_hundredth_of_a_tile_and_a_tenth_percent_zoom() {
    // A fragment lands exactly: only float text may differ.
    let base = camera(100.0, 1.0);
    assert!(base.same_view(&camera(100.004, 1.0005)));
    assert!(!base.same_view(&camera(100.02, 1.0)));
    assert!(!base.same_view(&camera(100.0, 1.01)));
}
