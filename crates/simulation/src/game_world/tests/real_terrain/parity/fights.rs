//! The two-sided parity scenes: fights whose outcome may not depend on who
//! looks (#978, 4.3, tests 3 and 7).
use super::*;
use crate::BuildingKind;
use aoe_core::{FIXED_SUBUNITS_PER_TILE, Formation, TileCoord, TileRect};

/// Remaining hit points of `ids` as a share of `start`.
fn left(world: &GameWorld, ids: &[EntityId], start: u32) -> f64 {
    let hp: u32 = ids
        .iter()
        .filter_map(|&id| world.unit(id))
        .map(|u| u32::from(u.hp))
        .sum();
    f64::from(hp) / f64::from(start.max(1))
}

/// Spawns `count` units of `kind` for `player` in a block of `columns`,
/// two tiles apart, from `(x, y)`. Returns them and their hit points.
fn block(
    world: &mut GameWorld,
    jitter: &mut Jitter,
    player: PlayerId,
    kind: UnitKind,
    count: i32,
    (x, y, columns): (i32, i32, i32),
) -> (Vec<EntityId>, u32) {
    let ids: Vec<_> = (0..count)
        .map(|n| {
            let at = jitter.at(x + 2 * (n % columns), y + 2 * (n / columns));
            world.spawn_kind(player, kind, at).unwrap()
        })
        .collect();
    let hp = ids
        .iter()
        .map(|&id| u32::from(world.unit(id).unwrap().hp))
        .sum();
    (ids, hp)
}

const FIGHT: [&str; 7] = [
    "pikes",
    "archers_a",
    "onagers",
    "knights",
    "archers_b",
    "side_a",
    "side_b",
];

/// Test 3: pikes, archers and onagers of player 1 against knights and
/// archers of player 2, both on attack-move through each other, on the open
/// east bank of the river fixture. Remaining hit points per unit class.
fn fight_with_an_edge(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    let mut world = build("river").world;
    let (a, b) = (PlayerId(1), PlayerId(2));
    let classes = [
        block(&mut world, jitter, a, UnitKind::Pikeman, 14, (130, 138, 4)),
        block(&mut world, jitter, a, UnitKind::Archer, 10, (124, 140, 3)),
        block(&mut world, jitter, a, UnitKind::Onager, 2, (120, 144, 1)),
        block(&mut world, jitter, b, UnitKind::Knight, 8, (176, 140, 4)),
        block(&mut world, jitter, b, UnitKind::Archer, 9, (188, 140, 3)),
    ];
    look.seat(&mut world, &[a, b]);
    let side = |range: std::ops::Range<usize>| -> Vec<EntityId> {
        classes[range]
            .iter()
            .flat_map(|(ids, _)| ids.clone())
            .collect()
    };
    world
        .issue_formation_move(
            a,
            &side(0..3),
            Formation::Line,
            center(196, 145),
            false,
            true,
        )
        .unwrap();
    world
        .issue_formation_move(
            b,
            &side(3..5),
            Formation::Line,
            center(116, 145),
            false,
            true,
        )
        .unwrap();
    for _ in 0..1_800 {
        world.advance();
    }
    let mut measures: Vec<f64> = classes
        .iter()
        .map(|(ids, hp)| left(&world, ids, *hp))
        .collect();
    for range in [0..3, 3..5] {
        let start = classes[range.clone()].iter().map(|(_, hp)| hp).sum();
        measures.push(left(&world, &side(range), start));
    }
    measures
}

#[test]
fn parity_3_a_fight_with_a_known_edge_does_not_depend_on_who_looks() {
    let result = compare("fight", &FIGHT, fight_with_an_edge);
    // The edge: player 1's pikes and onagers beat the knights.
    let ((knights, _), (pikes, _)) = (result.0.0[3], result.0.0[0]);
    assert!(
        pikes > knights,
        "pikes {pikes:.3} against knights {knights:.3}"
    );
    assert_two_sided("fight", &FIGHT, result);
}

const RAID: [&str; 2] = ["villagers_killed", "first_kill"];

/// Test 7: twelve light cavalry of player 1 attack-move down a wood line
/// where twenty villagers of player 2 cut wood. Villagers killed, and the
/// tick of the first kill as a share of the run.
fn raid(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    const TICKS: u32 = 600;
    let mut world = build("river").world;
    let (a, b) = (PlayerId(1), PlayerId(2));
    crate::game_world::terrain_fixtures::trees(&mut world, TileRect::from_xywh(200, 110, 4, 60));
    world
        .insert_building(
            b,
            BuildingKind::LumberCamp,
            TileRect::from_xywh(192, 138, 2, 2),
            true,
        )
        .unwrap();
    let nodes = world.starting_resources();
    let villagers: Vec<_> = (0..20)
        .map(|n| {
            let id = world
                .spawn_villager(b, jitter.at(198, 112 + 3 * n))
                .unwrap();
            let tile = TileCoord::new(200, 112 + 3 * n);
            let node = nodes.iter().find(|node| node.tile == tile).unwrap();
            world.issue_gather(b, id, node.id).unwrap();
            id
        })
        .collect();
    let (raiders, _) = block(
        &mut world,
        jitter,
        a,
        UnitKind::LightCavalry,
        12,
        (192, 90, 4),
    );
    look.seat(&mut world, &[a, b]);
    world
        .issue_formation_move(a, &raiders, Formation::Line, center(196, 185), false, true)
        .unwrap();
    let mut first = TICKS;
    for tick in 0..TICKS {
        world.advance();
        if first == TICKS
            && villagers
                .iter()
                .any(|&id| world.unit(id).is_none_or(|u| u.hp == 0))
        {
            first = tick;
        }
    }
    let killed = villagers
        .iter()
        .filter(|&&id| world.unit(id).is_none_or(|u| u.hp == 0))
        .count();
    vec![killed as f64 / 20.0, f64::from(first) / f64::from(TICKS)]
}

#[test]
fn parity_7_a_raid_on_a_wood_line_does_not_depend_on_who_looks() {
    let result = compare("raid", &RAID, raid);
    let (killed, _) = result.0.0[0];
    assert!(
        killed > 0.25,
        "the raid killed {killed:.3} of the villagers"
    );
    assert_two_sided("raid", &RAID, result);
}

const HOLD: [&str; 3] = ["defenders_lost", "attackers_lost", "crossing"];

/// Test 4: forty spearmen and archers of player 1 hold the dry gap in the
/// river fixture's river (x 88, y 82 to 94) against eighty militia of
/// player 2 on attack-move through it. Losses of each side, and the tick
/// the first attacker is well across, as a share of the run.
fn hold(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    const TICKS: u32 = 2_400;
    let mut world = build("river").world;
    let (a, b) = (PlayerId(1), PlayerId(2));
    let (spears, _) = block(&mut world, jitter, a, UnitKind::ManAtArms, 20, (94, 80, 4));
    let (archers, _) = block(&mut world, jitter, a, UnitKind::Archer, 20, (104, 80, 4));
    let (militia, _) = block(&mut world, jitter, b, UnitKind::Militia, 80, (40, 70, 8));
    look.seat(&mut world, &[a, b]);
    world
        .issue_formation_move(b, &militia, Formation::Line, center(140, 88), false, true)
        .unwrap();
    let mut crossed = TICKS;
    for tick in 0..TICKS {
        world.advance();
        if crossed == TICKS
            && militia
                .iter()
                .filter_map(|&id| world.unit(id))
                .any(|u| u.hp > 0 && u.position.x > 120 * FIXED_SUBUNITS_PER_TILE)
        {
            crossed = tick;
        }
    }
    let lost = |ids: &[EntityId]| {
        let dead = ids
            .iter()
            .filter(|&&id| world.unit(id).is_none_or(|u| u.hp == 0))
            .count();
        dead as f64 / ids.len() as f64
    };
    let defenders: Vec<_> = spears.iter().chain(&archers).copied().collect();
    vec![
        lost(&defenders),
        lost(&militia),
        f64::from(crossed) / f64::from(TICKS),
    ]
}

#[test]
fn parity_4_holding_a_crossing_does_not_depend_on_who_looks() {
    let result = compare("hold", &HOLD, hold);
    assert_two_sided("hold", &HOLD, result);
}

const TOWERS: [&str; 2] = ["damage_taken", "riders_lost"];

/// Test 5: twenty light cavalry of player 1 ride along the chunk border at
/// x = 160 past four watch towers of player 2, on alternate sides of it.
/// Damage taken as a share of their hit points, and riders lost. Most
/// arrows miss a moving rider, so the share is small.
fn past_towers(look: Look, jitter: &mut Jitter) -> Vec<f64> {
    let mut world = build("river").world;
    let (a, b) = (PlayerId(1), PlayerId(2));
    for (x, y) in [(157, 120), (162, 135), (157, 150), (162, 165)] {
        world
            .insert_building(
                b,
                BuildingKind::WatchTower,
                TileRect::from_xywh(x, y, 1, 1),
                true,
            )
            .unwrap();
    }
    let (riders, hp) = block(
        &mut world,
        jitter,
        a,
        UnitKind::LightCavalry,
        20,
        (157, 90, 4),
    );
    look.seat(&mut world, &[a, b]);
    world
        .issue_formation_move(a, &riders, Formation::Line, center(160, 200), false, false)
        .unwrap();
    for _ in 0..2_400 {
        world.advance();
    }
    let lost = riders
        .iter()
        .filter(|&&id| world.unit(id).is_none_or(|u| u.hp == 0))
        .count();
    vec![1.0 - left(&world, &riders, hp), lost as f64 / 20.0]
}

#[test]
fn parity_5_riding_past_towers_does_not_depend_on_who_looks() {
    let result = compare("towers", &TOWERS, past_towers);
    // Arrows mostly miss riders on the move, but some land.
    let (damage, _) = result.0.0[0];
    assert!(damage > 0.0, "the towers never hit the riders");
    assert_two_sided("towers", &TOWERS, result);
}
