use super::Index;
use crate::RiverSegment;

#[test]
fn ford_phase_survives_page_clipping_and_deep_overlaps_in_either_order() {
    let medium = RiverSegment {
        start: [128 * 256, 0],
        end: [128 * 256, 256 * 256],
        width_units: 6 * 256,
        station_start: 0,
        station_end: 256 * 256,
        source_delta: [0, 256 * 256],
        source_station_delta: 256 * 256,
    };
    let clipped = RiverSegment {
        start: [128 * 256, 123 * 256],
        station_start: 123 * 256,
        ..medium
    };
    let complete = Index::new(&[medium], [0, 0], 256 * 256);
    let page = Index::new(&[clipped], [0, 128 * 256], 128 * 256);
    for y in 128..250 {
        let point = [128 * 256, y * 256];
        let hit = complete.physical(point, 0, 0).unwrap();
        assert_eq!(hit, page.physical(point, 0, 0).unwrap());
        assert_eq!(
            hit.2, hit.1,
            "medium water wades only in the ford phase at {y}"
        );
    }
    let deep = RiverSegment {
        width_units: 7 * 256,
        ..medium
    };
    for segments in [[medium, deep], [deep, medium]] {
        let index = Index::new(&segments, [0, 0], 256 * 256);
        assert!(!index.physical([128 * 256, 160 * 256], 0, 0).unwrap().2);
    }
}

#[test]
fn page_clipped_bend_matches_the_unwarped_complete_curve() {
    let incoming = RiverSegment {
        start: [448 * 256, 256 * 256],
        end: [512 * 256, 256 * 256],
        width_units: 3 * 256,
        station_start: 128 * 256,
        station_end: 64 * 256,
        source_delta: [16384, 0],
        source_station_delta: -16384,
    };
    let outgoing = RiverSegment {
        start: incoming.end,
        end: [512 * 256, 320 * 256],
        source_delta: [0, 16384],
        station_start: 64 * 256,
        station_end: 0,
        ..incoming
    };
    let clipped = RiverSegment {
        start: [507 * 256, 256 * 256],
        station_start: 69 * 256,
        ..incoming
    };
    let complete = Index::new(&[incoming, outgoing], [0, 0], 512 * 256);
    let page = Index::new(&[clipped, outgoing], [512 * 256, 0], 512 * 256);
    let point = [130944, 65408];
    assert_eq!(complete.physical(point, 256, 0).unwrap().0, 315274);
    assert_eq!(complete.physical(point, 0, 0), None);
    assert_eq!(page.physical(point, 0, 0), None);
    for y in 252..=260 {
        for x in 508..=516 {
            let point = [x * 256 + 128, y * 256 + 128];
            assert_eq!(
                page.physical(point, 0, 0),
                complete.physical(point, 0, 0),
                "{point:?}"
            );
        }
    }
}
