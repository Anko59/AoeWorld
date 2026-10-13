use crate::ship::compare::pixels::{self, Image, Rect};

pub(super) fn solid(width: u32, height: u32, color: [u8; 4]) -> Image {
    Image::filled(width, height, color)
}

pub(super) fn paint(image: &mut Image, rect: Rect, color: [u8; 4]) {
    for y in rect.y..rect.y + rect.h {
        for x in rect.x..rect.x + rect.w {
            let i = (y as usize * image.width as usize + x as usize) * 4;
            image.rgba[i..i + 4].copy_from_slice(&color);
        }
    }
}

const WHOLE: Rect = Rect {
    x: 0,
    y: 0,
    w: 20,
    h: 10,
};

#[test]
fn identical_images_have_no_changed_pixels() {
    let image = solid(20, 10, [10, 20, 30, 255]);
    let diff = pixels::diff(&image, &image.clone(), WHOLE, &[]).expect("diff");
    assert_eq!(
        (diff.compared, diff.changed, diff.masked_changed),
        (200, 0, 0)
    );
    assert_eq!(diff.percent(), 0.0);
}

#[test]
fn a_known_changed_rectangle_counts_exactly() {
    let before = solid(20, 10, [0, 0, 0, 255]);
    let mut after = before.clone();
    paint(
        &mut after,
        Rect {
            x: 3,
            y: 2,
            w: 4,
            h: 3,
        },
        [0, 0, 1, 255],
    );
    let diff = pixels::diff(&before, &after, WHOLE, &[]).expect("diff");
    assert_eq!(diff.changed, 12);
    assert!((diff.percent() - 6.0).abs() < 1e-9, "12 of 200 pixels");
    let red = &diff.image.rgba[((2 * 20 + 3) * 4) as usize..][..4];
    assert_eq!(red, [255, 40, 40, 255], "changed pixels are highlighted");
}

#[test]
fn only_the_scene_is_counted_and_masks_are_reported_separately() {
    let before = solid(20, 10, [0, 0, 0, 255]);
    let mut after = before.clone();
    paint(
        &mut after,
        Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 1,
        },
        [9, 9, 9, 255],
    ); // outside the scene
    paint(
        &mut after,
        Rect {
            x: 12,
            y: 4,
            w: 2,
            h: 2,
        },
        [9, 9, 9, 255],
    ); // under a mask
    paint(
        &mut after,
        Rect {
            x: 2,
            y: 4,
            w: 1,
            h: 1,
        },
        [9, 9, 9, 255],
    ); // in the scene
    let scene = Rect {
        x: 0,
        y: 2,
        w: 20,
        h: 8,
    };
    let mask = Rect {
        x: 10,
        y: 3,
        w: 6,
        h: 4,
    };
    let diff = pixels::diff(&before, &after, scene, &[mask]).expect("diff");
    assert_eq!(diff.compared, 20 * 8 - 24);
    assert_eq!((diff.changed, diff.masked_changed), (1, 4));
    let only_hud = pixels::diff(
        &before,
        &after,
        scene,
        &[
            mask,
            Rect {
                x: 2,
                y: 4,
                w: 1,
                h: 1,
            },
        ],
    )
    .expect("diff");
    assert_eq!((only_hud.changed, only_hud.masked_changed), (0, 5));
}

#[test]
fn different_sizes_are_an_error_not_a_difference() {
    let error =
        pixels::diff(&solid(2, 2, [0; 4]), &solid(3, 2, [0; 4]), WHOLE, &[]).expect_err("sizes");
    assert!(error.contains("differ in size"), "{error}");
}

#[test]
fn pngs_round_trip_and_oversize_is_refused() {
    let mut image = solid(5, 4, [1, 2, 3, 255]);
    paint(
        &mut image,
        Rect {
            x: 1,
            y: 1,
            w: 2,
            h: 2,
        },
        [200, 100, 50, 255],
    );
    assert_eq!(
        Image::decode(&image.encode().expect("encode")).expect("decode"),
        image
    );
    assert!(Image::decode(b"not a png").is_err());
    let huge = solid(pixels::MAX_SIDE + 1, 1, [0; 4])
        .encode()
        .expect("encode");
    assert!(
        Image::decode(&huge)
            .expect_err("too wide")
            .contains("outside")
    );
}

#[test]
fn rows_are_one_to_one_and_the_contact_sheet_shrinks_by_whole_factors() {
    let a = solid(30, 20, [255, 0, 0, 255]);
    let b = solid(30, 20, [0, 255, 0, 255]);
    let row = pixels::row("alps", &[("BEFORE", &a), ("AFTER", &b)]);
    assert_eq!(
        (row.width, row.height),
        (30 + 6 + 30, pixels::LABEL_HEIGHT + 20)
    );
    let first = ((pixels::LABEL_HEIGHT * row.width) * 4) as usize;
    assert_eq!(
        &row.rgba[first..first + 4],
        [255, 0, 0, 255],
        "the first panel is unscaled"
    );
    let (sheet, factor) = pixels::stack(&[row.clone(), row.clone()]);
    assert_eq!(factor, 1);
    assert_eq!(sheet.width, row.width);
    let wide = solid(pixels::MONTAGE_LIMIT + 1, 2, [0; 4]);
    let (small, factor) = pixels::stack(&[wide]);
    assert_eq!(factor, 2);
    assert!(small.width <= pixels::MONTAGE_LIMIT);
}
