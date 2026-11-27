use aoe_core::{TileCoord, TileLine};

#[test]
fn raster_uses_bresenham_ties_in_every_drag_direction() {
    let start = TileCoord::new(20, 20);
    for end_x in 16..=24 {
        for end_y in 16..=24 {
            let end = TileCoord::new(end_x, end_y);
            let line = TileLine::new(start, end).unwrap();
            let mut pos = start;
            let dx = pos.x.abs_diff(end.x) as i32;
            let dy = -(pos.y.abs_diff(end.y) as i32);
            let sx = (end.x - pos.x).signum();
            let sy = (end.y - pos.y).signum();
            let mut error = dx + dy;
            for (index, tile) in line.tiles().enumerate() {
                assert_eq!(tile, pos, "{start:?} -> {end:?}, tile {index}");
                assert_eq!(line.index_of(tile), Some(index));
                if pos == end {
                    break;
                }
                let doubled = 2 * error;
                if doubled >= dy {
                    error += dy;
                    pos.x += sx;
                }
                if doubled <= dx {
                    error += dx;
                    pos.y += sy;
                }
            }
            assert_eq!(line.tile(line.count() - 1), end);
        }
    }
}
