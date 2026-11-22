//! Parity test 6 (#978, 4.3): an own column at its gate, then an enemy group
//! trying it, play out the same whoever looks.
use super::*;
use crate::BuildingKind;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, Formation, TileRect};

const GATE: [&str; 2] = ["own_through", "enemy_through"];

/// A palisade at y = 150 from the river to the map edge, with a gate
/// at x 158 to 161, closes the south-east of the river fixture. Ten
/// militia of player 1 march through their own gate; twenty of player 2
/// then try to follow. Since watching follows the screen (#978 step 3), the
/// unwatched run has no client showing the gate, enemy included.
fn gate(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    let mut world = build("river").world;
    let (a, b) = (PlayerId(1), PlayerId(2));
    let edge = world.config.width_tiles;
    // From inside the river (water tiles refuse a wall) to the map edge.
    for x in (84..edge).filter(|x| !(158..162).contains(x)) {
        let wall = TileRect::from_xywh(x, 150, 1, 1);
        let _ = world.insert_building(a, BuildingKind::PalisadeWall, wall, true);
    }
    let open: Vec<_> = (84..edge)
        .filter(|x| !(158..162).contains(x))
        .filter(|&x| world.tile_passable(aoe_core::TileCoord::new(x, 150)))
        .collect();
    assert!(open.is_empty(), "a way around the wall at x {open:?}");
    world
        .insert_building(
            a,
            BuildingKind::PalisadeGate,
            TileRect::from_xywh(158, 150, 4, 1),
            true,
        )
        .unwrap();
    let column: Vec<_> = (0..10)
        .map(|n| {
            world
                .spawn_militia(a, jitter.at(152 + 2 * (n % 5), 120 + 2 * (n / 5)))
                .unwrap()
        })
        .collect();
    let enemies: Vec<_> = (0..20)
        .map(|n| {
            world
                .spawn_militia(b, jitter.at(170 + 2 * (n % 5), 112 + 2 * (n / 5)))
                .unwrap()
        })
        .collect();
    look.seat(&mut world, &[a, b]);
    world
        .issue_formation_move(a, &column, Formation::Line, center(160, 180), false, false)
        .unwrap();
    for tick in 0..2_400 {
        if tick == 1_400 {
            // The enemy may find no way in: that is the point.
            let _ = world.issue_formation_move(
                b,
                &enemies,
                Formation::Line,
                center(170, 185),
                false,
                false,
            );
        }
        world.advance();
    }
    let through = |ids: &[EntityId]| {
        let south = ids
            .iter()
            .filter_map(|&id| world.unit(id))
            .filter(|u| u.hp > 0 && u.position.y > 151 * FIXED_SUBUNITS_PER_TILE)
            .count();
        south as f64 / ids.len() as f64
    };
    vec![through(&column), through(&enemies)]
}

#[test]
#[ignore = "#999: watched, the column stops at its own gate and holds it open"]
fn parity_6_a_gate_crossing_does_not_depend_on_who_looks() {
    // Watched, the column stops in front of its gate and holds it open, so
    // the enemy gets in (#999); unwatched it passes (own 100%, enemy 5%).
    let result = compare("gate", &GATE, gate);
    assert_two_sided("gate", &GATE, result);
}
