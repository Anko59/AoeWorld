use super::Index;
use crate::RiverSegment;

#[test]
fn page_clipped_bend_matches_the_unwarped_complete_curve() {
    let incoming = RiverSegment {
        start: [448 * 256, 256 * 256],
        end: [512 * 256, 256 * 256],
        width_units: 3 * 256,
        station_start: 128 * 256,
        station_end: 64 * 256,
        source_delta: [64 * 256, 0],
        source_station_delta: -64 * 256,
    };
    let outgoing = RiverSegment {
        start: incoming.end,
        end: [512 * 256, 320 * 256],
        source_delta: [0, 64 * 256],
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
            assert_eq!(page.physical(point, 0, 0), complete.physical(point, 0, 0), "{point:?}");
        }
    }
}
