//! Truth for the owner's first real-map run, and the map's class grid for choosing
//! march pairs. Local output only, in `.cache/armies/france`.
use super::*;
use aoe_map::RouteTerrain;
use std::fmt::Write as _;
use std::time::Instant;

const START: TileCoord = TileCoord::new(996, 1040);
const GOAL: TileCoord = TileCoord::new(1106, 930);

/// 0 land, 1 river, 2 other water, 3 trees or other blocked land, 4 wet but walkable.
pub(super) fn class(world: &GameWorld, tile: TileCoord) -> u8 {
    let Terrain::Map { generator, overlay } = world.terrain() else {
        return 0;
    };
    let walkable = generator
        .route_walkable(overlay, tile, &|| false)
        .unwrap_or(false);
    let sample = generator.tile_and_node_with_cancel(tile, &|| false);
    let water = sample
        .ok()
        .flatten()
        .map_or(aoe_map::WaterKind::None, |(sample, _)| sample.water);
    match (walkable, water) {
        (true, aoe_map::WaterKind::None) => 0,
        (true, _) => 4,
        (false, aoe_map::WaterKind::River) => 1,
        (false, aoe_map::WaterKind::None) => 3,
        (false, _) => 2,
    }
}

fn ascii(world: &GameWorld, min: TileCoord, max: TileCoord, marks: &[(TileCoord, char)]) -> String {
    let mut out = String::new();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let tile = TileCoord::new(x, y);
            let mark = marks.iter().find(|(at, _)| *at == tile).map(|m| m.1);
            out.push(mark.unwrap_or(['.', '~', '=', 'T', ','][usize::from(class(world, tile))]));
        }
        out.push('\n');
    }
    out
}

#[test]
fn owner_goal_truth() {
    let Some(_) = label() else { return };
    let began = Instant::now();
    let mut world = france();
    let mut out = format!("load {:?}\n", began.elapsed());
    let side = world.config().width_tiles;
    let wide = |world: &GameWorld| field(world, GOAL, START, side);
    let began = Instant::now();
    let without = wide(&world);
    let _ = writeln!(
        out,
        "without bridge: start-to-goal {:?} tiles ({:?})",
        without.cost(START).map(|c| c / 1024),
        began.elapsed()
    );
    world
        .place_finished_bridge(span((1016, 1020), (1021, 1015)))
        .unwrap();
    let with = wide(&world);
    let _ = writeln!(
        out,
        "with bridge: start-to-goal {:?} tiles; bridge mouths {:?} {:?}",
        with.cost(START).map(|c| c / 1024),
        with.cost(TileCoord::new(1016, 1020)).map(|c| c / 1024),
        with.cost(TileCoord::new(1021, 1015)).map(|c| c / 1024),
    );
    let mut at = START;
    let mut trail = vec![(START, '@')];
    for _ in 0..200_000 {
        let best = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| TileCoord::new(at.x + dx, at.y + dy)))
            .filter_map(|t| with.cost(t).map(|c| (c, t.y, t.x)))
            .min();
        let Some((cost, y, x)) = best else { break };
        if cost >= with.cost(at).unwrap_or(0) {
            break;
        }
        at = TileCoord::new(x, y);
        trail.push((at, '*'));
    }
    let _ = writeln!(out, "descent steps {} ends {:?}", trail.len(), at);
    for (i, (t, _)) in trail.iter().enumerate().step_by(40) {
        let _ = writeln!(
            out,
            "  {i}: {:?} left {:?}",
            t,
            with.cost(*t).map(|c| c / 1024)
        );
    }
    trail.push((GOAL, 'G'));
    let min = TileCoord::new(960, 860);
    let max = TileCoord::new(1200, 1120);
    out += &ascii(&world, min, max, &trail);
    std::fs::write(output().join("owner-truth.txt"), out).unwrap();
}

/// The whole map, one class byte a tile, row by row.
#[test]
fn class_grid() {
    let Some(_) = label() else { return };
    let world = france();
    let side = world.config().width_tiles;
    let began = Instant::now();
    let mut grid = Vec::with_capacity((side * side) as usize);
    for y in 0..side {
        for x in 0..side {
            grid.push(class(&world, TileCoord::new(x, y)));
        }
    }
    std::fs::write(output().join("classes.bin"), grid).unwrap();
    std::fs::write(
        output().join("classes.txt"),
        format!("{side} {:?}\n", began.elapsed()),
    )
    .unwrap();
}
