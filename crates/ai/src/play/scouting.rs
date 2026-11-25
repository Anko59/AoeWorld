//! Public destinations for missions (`ai/aoe_ai/play/scouting.py`). The scouts
//! themselves (`ScoutManager`) are a later task.
use aoe_sdk::Visibility;

use crate::context::Context;
use crate::geometry::clamp_tile;
use crate::view::Tile;

/// Pick a public destination and let the server plan the full route.
pub fn mission_destination(ctx: &Context<'_>, goal: Tile) -> Option<Tile> {
    for radius in [0, 2, 4, 8] {
        let offsets = if radius == 0 {
            vec![(0, 0)]
        } else {
            vec![
                (radius, 0),
                (-radius, 0),
                (0, radius),
                (0, -radius),
                (radius, radius),
                (-radius, radius),
                (radius, -radius),
                (-radius, -radius),
            ]
        };
        for (dx, dy) in offsets {
            let tile = clamp_tile((goal.0 + dx, goal.1 + dy), ctx.world.map_size(), 3);
            if ctx.board.bad_moves.contains(&tile) {
                continue;
            }
            if ctx.world.visibility(tile) == Visibility::Unexplored
                || ctx.world.passable(tile) != Some(false)
            {
                return Some(tile);
            }
        }
    }
    None
}
