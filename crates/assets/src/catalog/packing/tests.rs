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

#[test]
fn logical_page_budget_can_exceed_the_current_runtime_atlas() {
    let frames = [object(2044, 2044); 6];
    let placed = pack_frames_with_pages(&frames, 2, 6).unwrap();
    assert_eq!(
        placed.iter().map(|p| p.page).collect::<Vec<_>>(),
        [2, 3, 4, 5, 6, 7]
    );
    assert!(pack_frames_with_pages(&frames, 2, 5).is_err());
    assert!(pack_frames_with_pages(&frames, 2, MAX_LOGICAL_PAGES).is_err());
    let mut dense_placements = vec![Placement::default(); frames.len()];
    dense::pack(&frames, &[0, 1, 2, 3, 4, 5], &mut dense_placements, 2, 8).unwrap();
    assert_eq!(dense_placements, placed);
    for (terrain, objects) in [(0, 1), (1, 0), (usize::MAX, 1), (1, MAX_LOGICAL_PAGES)] {
        assert_eq!(
            pack_with_allocator_and_pages(
                &frames,
                terrain,
                objects,
                MAX_LOGICAL_FRAMES,
                |_| panic!("allocated")
            ),
            Err(PackingError::InvalidPageBudget)
        );
    }
}

#[test]
fn logical_atlas_accepts_more_frames_than_the_resident_atlas() {
    let frames = vec![object(1, 1); 10_000];
    let placed = pack_frames_with_pages(&frames, 2, 6).unwrap();
    assert_eq!(placed.len(), frames.len());
    assert!(placed.iter().all(|p| p.page >= 2 && p.page < 8));
    assert!(matches!(
        pack_frames(&frames),
        Err(PackingError::TooManyFrames { .. })
    ));
}

#[test]
fn logical_frame_indices_cover_the_all_unit_inventory_and_reject_excess() {
    let frames = vec![object(1, 1); MAX_LOGICAL_FRAMES];
    let placed = pack_frames_with_pages(&frames, 1, 1).unwrap();
    assert_eq!(placed.len(), MAX_LOGICAL_FRAMES);
    assert_ne!(placed[0], placed[MAX_LOGICAL_FRAMES - 1]);
    let excess = vec![object(1, 1); MAX_LOGICAL_FRAMES + 1];
    assert_eq!(
        pack_with_allocator_and_pages(&excess, 1, 1, MAX_LOGICAL_FRAMES, |_| panic!("allocated")),
        Err(PackingError::TooManyFrames {
            count: MAX_LOGICAL_FRAMES + 1,
            limit: MAX_LOGICAL_FRAMES
        })
    );
}

#[test]
fn ordering_large_reverse_catalog_has_bounded_comparisons() {
    let frames: Vec<_> = (0..MAX_SELECTED_FRAMES)
        .map(|i| object(1, (i % 2044 + 1) as u16))
        .collect();
    let mut indices: Vec<u32> = (0..frames.len() as u32).collect();
    let mut comparisons = 0;
    order_indices(&frames, &mut indices, || comparisons += 1);
    assert!(
        comparisons <= frames.len() * 32,
        "{comparisons} comparisons"
    );
    assert!(
        indices
            .windows(2)
            .all(|pair| precedes(&frames, pair[0] as usize, pair[1] as usize))
    );
}

fn reject_before_allocation(frames: &[FrameExtent], error: PackingError) {
    assert_eq!(
        pack_with_allocator(frames, |_| panic!("allocated before validation")),
        Err(error)
    );
}

#[test]
fn fixed_payload_and_explicit_white_address() {
    assert_eq!((PAGE_COUNT, PAGE_SIDE), (5, 2048));
    assert_eq!(PAGE_BYTES, 16_777_216);
    assert_eq!(ATLAS_BYTES, 83_886_080);
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
fn complete_actor_animation_sets_can_use_three_object_pages() {
    let placed = pack_frames(&[object(2044, 2044); 3]).unwrap();
    assert_eq!(placed.iter().map(|p| p.page).collect::<Vec<_>>(), [2, 3, 4]);
    assert!(pack_frames(&[object(2044, 2044); 4]).is_err());
}

#[test]
fn later_castle_frames_fill_the_unused_width_of_earlier_tall_shelves() {
    let mut frames = vec![object(1300, 1300); 3];
    frames.extend([object(1000, 700); 6]);
    frames.extend([object(700, 700); 3]);
    // The old single cursor abandoned a 745-pixel-wide gap beside each tall
    // frame, then overflowed at the first small frame despite ample free area.
    // Reuse those holes inside the same three object pages and pixel budget.
    let placed = pack_frames(&frames).unwrap();
    assert_eq!(
        placed[9..].iter().map(|p| p.page).collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert!(placed[9..].iter().all(|p| (p.x, p.y) == (1303, 2)));
    for (index, p) in placed.iter().enumerate() {
        for q in &placed[..index] {
            assert!(
                p.page != q.page
                    || u32::from(p.x) + u32::from(p.width) < u32::from(q.x)
                    || u32::from(q.x) + u32::from(q.width) < u32::from(p.x)
                    || u32::from(p.y) + u32::from(p.height) < u32::from(q.y)
                    || u32::from(q.y) + u32::from(q.height) < u32::from(p.y)
            );
        }
    }
    assert_eq!(ATLAS_BYTES, 83_886_080);
}

#[test]
fn unowned_art_borrows_spare_terrain_space_without_displacing_masked_objects() {
    let mut shared = object(2044, 2044);
    shared.domain = AtlasDomain::Shared;
    let frames = [
        shared,
        object(2044, 2044),
        terrain(2044, 2044),
        object(2044, 2044),
        object(2044, 2044),
    ];
    let placed = pack_frames(&frames).unwrap();
    // Shared appears first in semantic order, but must leave both hard domains
    // their space before using the spare terrain page. No new pixel storage.
    assert_eq!(
        placed.iter().map(|p| p.page).collect::<Vec<_>>(),
        [1, 2, 0, 3, 4]
    );
    assert_eq!(ATLAS_BYTES, 83_886_080);
    let mut full = frames.to_vec();
    full.push(object(1, 1));
    assert!(matches!(
        pack_frames(&full),
        Err(PackingError::DomainOverflow {
            domain: AtlasDomain::Objects,
            ..
        })
    ));
}

#[test]
fn count_limit_distinguishes_the_full_animation_budget_from_one_excess_frame() {
    let calls = Cell::new(0);
    let frames = vec![terrain(1, 1); MAX_SELECTED_FRAMES];
    let placements = pack_with_allocator(&frames, |count| {
        calls.set(calls.get() + 1);
        assert_eq!(count, MAX_SELECTED_FRAMES);
        vec![Placement::default(); count]
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(placements.len(), MAX_SELECTED_FRAMES);
    assert!(placements.iter().all(|p| p.page == 0));
    let excess = vec![terrain(1, 1); MAX_SELECTED_FRAMES + 1];
    reject_before_allocation(
        &excess,
        PackingError::TooManyFrames {
            count: MAX_SELECTED_FRAMES + 1,
            limit: MAX_SELECTED_FRAMES,
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
        pack_frames(&[
            object(2044, 2044),
            object(2044, 2044),
            object(2044, 2044),
            object(1, 1)
        ]),
        Err(PackingError::DomainOverflow {
            index: 3,
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
            AtlasDomain::Shared => assert!((p.page as usize) < PAGE_COUNT),
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

#[test]
fn tall_unowned_art_uses_vertical_slivers_between_short_terrain_rows() {
    let mut frames = vec![object(2044, 2044); 3];
    frames.extend([terrain(1000, 100); 80]);
    frames.push(FrameExtent {
        domain: AtlasDomain::Shared,
        width: 40,
        height: 600,
    });
    // All masked pages are occupied. Packing short terrain rows first leaves
    // a continuous 43px-wide side sliver that shelves cannot reuse. Dense packing fits
    // unchanged art on the same two terrain pages, respecting every hard domain.
    let placed = pack_frames(&frames).unwrap();
    assert_eq!(placed.len(), frames.len());
    assert!(placed[..3].iter().all(|p| (2..5).contains(&p.page)));
    assert!(placed[3..].iter().all(|p| p.page < 2));
    for (index, p) in placed.iter().enumerate() {
        assert!(p.x >= 2 && p.y >= 2);
        assert!(p.x + p.width + 1 < PAGE_SIDE && p.y + p.height + 1 < PAGE_SIDE);
        for q in &placed[..index] {
            assert!(
                p.page != q.page
                    || p.x + p.width < q.x
                    || q.x + q.width < p.x
                    || p.y + p.height < q.y
                    || q.y + q.height < p.y
            );
        }
    }
    assert_eq!(ATLAS_BYTES, 83_886_080);
}

#[test]
fn dense_retry_keeps_mixed_rectangles_disjoint_and_deterministic() {
    let frames: Vec<_> = (0..240)
        .map(|i| FrameExtent {
            domain: match i % 3 {
                0 => AtlasDomain::Terrain,
                1 => AtlasDomain::Objects,
                _ => AtlasDomain::Shared,
            },
            width: 30 + (i * 37 % 160) as u16,
            height: 20 + (i * 53 % 120) as u16,
        })
        .collect();
    let indices: Vec<_> = (0..frames.len() as u32).collect();
    let mut placed = vec![Placement::default(); frames.len()];
    dense::pack(&frames, &indices, &mut placed, 2, PAGE_COUNT).unwrap();
    let mut repeat = placed.clone();
    dense::pack(&frames, &indices, &mut repeat, 2, PAGE_COUNT).unwrap();
    assert_eq!(placed, repeat);
    for (i, p) in placed.iter().enumerate() {
        assert_eq!((p.width, p.height), (frames[i].width, frames[i].height));
        assert!(p.x >= 2 && p.y >= 2);
        assert!(p.x + p.width + 1 < PAGE_SIDE && p.y + p.height + 1 < PAGE_SIDE);
        match frames[i].domain {
            AtlasDomain::Terrain => assert!(p.page < 2),
            AtlasDomain::Objects => assert!((2..5).contains(&p.page)),
            AtlasDomain::Shared => assert!(p.page < 5),
        }
        for q in &placed[..i] {
            assert!(
                p.page != q.page
                    || p.x + p.width < q.x
                    || q.x + q.width < p.x
                    || p.y + p.height < q.y
                    || q.y + q.height < p.y
            );
        }
    }
}
