//! At the Steward's scale the Vassal walls the domain's land approaches with
//! gates along each side's axis, towers its work points, and its soldiers
//! answer a raid at the domain edge. Prints the measured counts.
use super::*;

fn count(w: &GameWorld, kind: BuildingKind) -> usize {
    w.buildings()
        .filter(|b| b.player == PlayerId(0) && b.kind == kind && b.is_complete())
        .count()
}

#[test]
fn vassal_fortifies_a_steward_sized_domain_and_defends_its_edge() {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile {
        food: 100_000,
        wood: 100_000,
        gold: 100_000,
        stone: 100_000,
    };
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    for i in 0..20 {
        w.spawn_villager(PlayerId(0), at(86 + i % 10, 97 + i / 10))
            .unwrap();
    }
    for i in 0..6 {
        w.spawn_militia(PlayerId(0), at(88 + i, 86)).unwrap();
    }
    let center = TileCoord::new(92, 92);
    let ring = crate::VASSAL_RADIUS * 7 / 10;
    let mut ticks = 0;
    while ticks < 40_000 && !w.vassal_status(v).unwrap().developed {
        advance(&mut w, 200);
        ticks += 200;
    }
    let status = w.vassal_status(v).unwrap();
    let walls = count(&w, BuildingKind::StoneWall) + count(&w, BuildingKind::PalisadeWall);
    let gates: Vec<_> = w
        .buildings()
        .filter(|b| b.kind.is_gate() && b.is_complete())
        .map(|b| b.footprint)
        .collect();
    let vertical = gates.iter().filter(|r| r.height() == 4).count();
    let towers = count(&w, BuildingKind::WatchTower);
    eprintln!(
        "vassal domain after {ticks} ticks: {walls} wall tiles, {} gates ({vertical} vertical), \
         {towers} towers, {}/{} sites, wait {:?}",
        gates.len(),
        status.completed,
        status.total,
        status.wait
    );
    // Measured 397 of the ring's 541 wall tiles at 40,000 ticks with 20 builders.
    assert!(walls >= 350, "walls {walls}");
    assert!(gates.len() >= 8 && vertical >= 3, "gates {gates:?}");
    assert!(towers >= 2, "towers {towers}");
    // Gates sit in their side: horizontal ones on the north/south lines,
    // vertical ones on the west/east lines.
    for g in &gates {
        if g.height() == 4 {
            assert_eq!((g.min.x - center.x).abs(), ring, "{g:?}");
        } else {
            assert_eq!((g.min.y - center.y).abs(), ring, "{g:?}");
        }
    }
    // A villager from outside the wall walks in through a gate.
    let outside = w
        .spawn_villager(PlayerId(0), at(center.x + 3, center.y + ring + 8))
        .unwrap();
    let goal = TileCoord::new(center.x + 3, center.y + ring - 8);
    w.issue_move(outside, WorldPosition::from_tile_center(goal).unwrap())
        .unwrap();
    let mut crossed = None;
    for t in 0..1_200 {
        w.advance();
        if w.unit(outside).unwrap().position.tile_floor() == goal {
            crossed = Some(t);
            break;
        }
    }
    eprintln!("outsider crossed the wall in {crossed:?} ticks");
    assert!(crossed.is_some(), "{:?}", w.unit(outside).unwrap().position);
    // A raider inside the domain edge raises the alert and draws the soldiers.
    let raider = w
        .spawn_militia(PlayerId(1), at(center.x + 20, center.y - ring + 4))
        .unwrap();
    let mut answered = None;
    for t in 0..600 {
        w.advance();
        let attacking = w
            .units()
            .filter(|u| u.player == PlayerId(0) && w.vassal_controller(u.id) == Some(v))
            .filter(|u| u.kind != UnitKind::Villager && u.task == UnitTask::Attacking)
            .count();
        if w.vassal_status(v).unwrap().alert && attacking > 0 {
            answered = Some((t, attacking));
            break;
        }
    }
    eprintln!("raid at the edge answered: {answered:?}");
    assert!(
        answered.is_some(),
        "raider {:?}",
        w.unit(raider).map(|u| u.position)
    );
}

#[test]
fn vassal_soldiers_kill_a_wolf_hunting_a_builder() {
    let (mut w, v, _) = setup();
    w.set_fog_disabled(true);
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let builder = w.spawn_villager(PlayerId(0), at(84, 100)).unwrap();
    for i in 0..4 {
        w.spawn_militia(PlayerId(0), at(88 + i, 98)).unwrap();
    }
    advance(&mut w, 40);
    assert_eq!(w.vassal_controller(builder), Some(v));
    let wolf = w.spawn_animal(crate::AnimalKind::Wolf, at(82, 99)).unwrap();
    let mut killed = None;
    for t in 0..1_200 {
        w.advance();
        if w.animal(wolf).is_none_or(|a| a.hp == 0) {
            killed = Some(t);
            break;
        }
    }
    eprintln!(
        "wolf killed after {killed:?} ticks; builder hp {:?}",
        w.unit(builder).map(|u| u.hp)
    );
    assert!(killed.is_some(), "the soldiers must kill the wolf");
    assert!(
        w.unit(builder).is_some_and(|u| u.hp > 0),
        "the builder lives"
    );
}
