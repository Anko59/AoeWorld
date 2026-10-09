use super::*;
use std::cell::Cell;

fn terrain(width: u16, height: u16) -> FrameExtent {
    FrameExtent {
        domain: AtlasDomain::Terrain,
        width,
        height,
    }
}

fn object(width: u16, height: u16) -> FrameExtent {
    FrameExtent {
        domain: AtlasDomain::Objects,
        width,
        height,
    }
}

fn reject_before_allocation(frames: &[FrameExtent], error: PackingError) {
    assert_eq!(
        pack_with_allocator(frames, |_| panic!("allocated before validation")),
        Err(error)
    );
}

#[test]
fn fixed_payload_and_explicit_white_address() {
    assert_eq!((PAGE_COUNT, PAGE_SIDE), (3, 2048));
    assert_eq!(PAGE_BYTES, 16_777_216);
    assert_eq!(ATLAS_BYTES, 50_331_648);
    assert_eq!(
        WHITE_TEXEL,
        Placement {
            page: 2,
            x: 0,
            y: 0,
            width: 1,
            height: 1
        }
    );
    assert!(pack_frames(&[]).unwrap().is_empty());
}

#[test]
fn count_limit_distinguishes_2048_allocation_from_2049_rejection() {
    let calls = Cell::new(0);
    let frames = vec![terrain(1, 1); MAX_SELECTED_FRAMES];
    let placements = pack_with_allocator(&frames, |count| {
        calls.set(calls.get() + 1);
        assert_eq!(count, 2048);
        vec![Placement::default(); count]
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(placements.len(), 2048);
    assert!(placements.iter().all(|p| p.page == 0));
    let excess = vec![terrain(1, 1); MAX_SELECTED_FRAMES + 1];
    reject_before_allocation(
        &excess,
        PackingError::TooManyFrames {
            count: 2049,
            limit: 2048,
        },
    );
}

#[test]
fn exact_2044_fit_and_2045_oversize_on_either_axis() {
    assert_eq!(
        pack_frames(&[terrain(2044, 2044)]).unwrap(),
        vec![Placement {
            page: 0,
            x: 2,
            y: 2,
            width: 2044,
            height: 2044
        }]
    );
    for (width, height) in [(2045, 1), (1, 2045), (u16::MAX, u16::MAX)] {
        reject_before_allocation(
            &[terrain(width, height)],
            PackingError::OversizedExtent {
                index: 0,
                width,
                height,
            },
        );
    }
}

#[test]
fn zero_extent_on_either_axis_is_explicit() {
    for (width, height) in [(0, 1), (1, 0), (0, 0)] {
        reject_before_allocation(
            &[object(width, height)],
            PackingError::ZeroExtent { index: 0 },
        );
    }
}

#[test]
fn validates_entire_input_before_allocating_or_attempting_overflow() {
    reject_before_allocation(
        &[object(2044, 2044), object(2044, 2044), terrain(2045, 1)],
        PackingError::OversizedExtent {
            index: 2,
            width: 2045,
            height: 1,
        },
    );
}

#[test]
fn terrain_spills_to_one_but_never_borrows_two() {
    let frames = [terrain(2044, 2044); 2];
    let placements = pack_frames(&frames).unwrap();
    assert_eq!(
        placements.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        pack_frames(&[terrain(2044, 2044); 3]),
        Err(PackingError::DomainOverflow {
            index: 2,
            domain: AtlasDomain::Terrain
        })
    );
}

#[test]
fn object_domain_overflows_while_both_terrain_pages_are_empty() {
    assert_eq!(
        pack_frames(&[object(2044, 2044), object(1, 1)]),
        Err(PackingError::DomainOverflow {
            index: 1,
            domain: AtlasDomain::Objects
        })
    );
}

#[test]
fn deterministic_height_width_ties_preserve_semantic_indices() {
    let frames = [terrain(5, 5), object(9, 9), terrain(10, 5), terrain(5, 5)];
    let expected = vec![
        Placement {
            page: 0,
            x: 13,
            y: 2,
            width: 5,
            height: 5,
        },
        Placement {
            page: 2,
            x: 2,
            y: 2,
            width: 9,
            height: 9,
        },
        Placement {
            page: 0,
            x: 2,
            y: 2,
            width: 10,
            height: 5,
        },
        Placement {
            page: 0,
            x: 19,
            y: 2,
            width: 5,
            height: 5,
        },
    ];
    for _ in 0..4 {
        assert_eq!(pack_frames(&frames).unwrap(), expected);
    }
}

#[test]
fn descending_height_precedes_width() {
    let frames = [terrain(100, 5), terrain(10, 6), terrain(20, 6)];
    let placements = pack_frames(&frames).unwrap();
    assert_eq!(
        placements.iter().map(|p| p.x).collect::<Vec<_>>(),
        vec![34, 23, 2]
    );
}

#[test]
fn failed_last_row_trial_preserves_space_for_a_smaller_frame() {
    let mut cursor = Cursor {
        x: 1800,
        y: 1900,
        row_height: 100,
    };
    let before = cursor;
    assert_eq!(cursor.place(300, 120), None);
    assert_eq!(cursor, before);
    assert_eq!(cursor.place(100, 100), Some((1800, 1900)));
}

#[test]
fn failed_first_page_trial_does_not_discard_smaller_remaining_frames() {
    let placements =
        pack_frames(&[terrain(1800, 1900), terrain(300, 150), terrain(200, 100)]).unwrap();
    assert_eq!(
        placements.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!((placements[2].x, placements[2].y), (1803, 2));
}

#[test]
fn domains_gutters_row_gap_and_no_overlap() {
    let frames = [
        terrain(1021, 1021),
        object(1021, 1021),
        terrain(1021, 1021),
        object(1021, 1021),
        terrain(1021, 1021),
        terrain(1021, 1021),
        terrain(1021, 1021),
        object(1021, 1021),
        object(1021, 1021),
    ];
    let placements = pack_frames(&frames).unwrap();
    assert_eq!((placements[4].x, placements[4].y), (2, 1024));
    assert_eq!(placements[6].page, 1);
    for (index, p) in placements.iter().enumerate() {
        match frames[index].domain {
            AtlasDomain::Terrain => assert!(p.page < 2),
            AtlasDomain::Objects => assert_eq!(p.page, 2),
        }
        assert_eq!(
            (p.width, p.height),
            (frames[index].width, frames[index].height)
        );
        assert!(p.x >= 2 && p.y >= 2);
        assert!(u32::from(p.x) + u32::from(p.width) + 1 < u32::from(PAGE_SIDE));
        assert!(u32::from(p.y) + u32::from(p.height) + 1 < u32::from(PAGE_SIDE));
        for q in &placements[..index] {
            if p.page != q.page {
                continue;
            }
            let separated = u32::from(p.x) + u32::from(p.width) < u32::from(q.x)
                || u32::from(q.x) + u32::from(q.width) < u32::from(p.x)
                || u32::from(p.y) + u32::from(p.height) < u32::from(q.y)
                || u32::from(q.y) + u32::from(q.height) < u32::from(p.y);
            assert!(separated, "overlap or missing gutter: {p:?} versus {q:?}");
        }
    }
}
