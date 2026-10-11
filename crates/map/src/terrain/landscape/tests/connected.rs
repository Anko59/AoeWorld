use super::*;
use crate::landscape_patches::Patches;
use std::collections::VecDeque;

#[test]
fn the_central_glade_links_to_the_trail_node_of_its_cell() {
    for key in [3_u8, 17, 71, 255] {
        for width in [1_024, 70_000] {
            let terrain = MapChunkGenerator::new([key; 32], 600, width);
            let patches = Patches::new([key; 32], 600);
            let coordinate = (width - 1) / 2;
            let center = TileCoord::new(coordinate, coordinate);
            assert!(starting_glade(&terrain, center));
            assert!(terrain.landscape_reservations_at(center).route);
            // Flood the connector from the centre: one connected corridor in
            // tile space although it is defined in warped space.
            let (side, origin) = (480_i32, coordinate - 240);
            let mut seen = vec![false; (side * side) as usize];
            let mut queue = VecDeque::from([center]);
            seen[((center.y - origin) * side + center.x - origin) as usize] = true;
            let mut reached = Vec::new();
            while let Some(tile) = queue.pop_front() {
                reached.push(patches.warped(i64::from(tile.x), i64::from(tile.y)));
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    let (x, y) = (next.x - origin, next.y - origin);
                    if x < 0 || y < 0 || x >= side || y >= side {
                        continue;
                    }
                    let index = (y * side + x) as usize;
                    if !seen[index] && starting_connector_contains(&terrain, next) {
                        seen[index] = true;
                        queue.push_back(next);
                    }
                }
            }
            let start = patches.warped(i64::from(center.x), i64::from(center.y));
            let node = patches.node(start.0.div_euclid(192), start.1.div_euclid(192));
            assert!(
                reached
                    .iter()
                    .any(|(x, y)| (x - node.0).abs() <= 2 && (y - node.1).abs() <= 2),
                "key={key} width={width}: connector does not reach the node"
            );
        }
    }
}
