use super::*;
#[test]
fn frame_cost_records_work_and_reset_cannot_leak_between_frames() {
    take();
    add(Counter::TexturedTriangles, 17);
    add(Counter::TexturedTriangles, 3);
    add(Counter::TerrainDemandTiles, 9);
    assert_eq!(take(), [9, 20, 0, 0, 0, 0]);
    assert_eq!(take(), [0; 6]);
    assert_eq!(NAMES.split(',').count(), take().len());
}
