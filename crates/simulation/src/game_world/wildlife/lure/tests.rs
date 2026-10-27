use crate::{AnimalKind, GameWorld, UnitTask};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// The observable stage of a shepherd, as the client draws it.
fn phase(w: &GameWorld, worker: aoe_core::EntityId, sheep: aoe_core::EntityId) -> &'static str {
    let u = w.unit(worker).unwrap();
    let a = w.animal(sheep);
    let alive = a.is_some_and(|a| a.hp > 0);
    match u.task {
        UnitTask::Moving if alive => "walk",
        // A shepherd whose route is still being scheduled is waiting, not killing.
        _ if u.planning => "wait",
        UnitTask::Shepherding if alive && u.moving => "herd",
        UnitTask::Shepherding if alive => "kill",
        UnitTask::Shepherding if !u.moving && u.carried.is_some() => "poach",
        UnitTask::Returning => "return",
        _ => "other",
    }
}

#[test]
fn shepherd_walks_leads_the_sheep_to_the_town_center_kills_poaches_and_delivers() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.place_town_center(PlayerId(0), TileCoord::new(95, 95))
        .unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(118, 97)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(113, 97)).unwrap();
    w.advance();
    let start = w.animal(sheep).unwrap().position;
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    let mut phases: Vec<&str> = Vec::new();
    let mut killed_at = None;
    let mut deliveries = 0;
    let mut food = 0;
    for _ in 0..20 * 600 {
        w.advance();
        let p = phase(&w, worker, sheep);
        if p == "walk" {
            assert_eq!(
                w.animal(sheep).unwrap().position,
                start,
                "the sheep waits until its shepherd reaches it"
            );
        }
        // A shepherd waiting for its scheduled route neither strikes nor
        // herds; the sheep is unharmed until it has been led.
        if !phases.contains(&"herd") {
            assert_eq!(w.animal(sheep).unwrap().hp, 7, "struck before leading");
        }
        if killed_at.is_none() && w.animal(sheep).is_some_and(|a| a.hp == 0) {
            killed_at = Some(w.animal(sheep).unwrap().position);
        }
        let now = w.player_economy(PlayerId(0)).unwrap().stockpile.food;
        if now > food {
            deliveries += 1;
            food = now;
        }
        if !matches!(p, "other" | "wait") && phases.last() != Some(&p) {
            phases.push(p);
        }
    }
    assert_eq!(
        &phases[..6],
        &["walk", "herd", "kill", "poach", "return", "poach"],
        "{phases:?}"
    );
    // The sheep was slaughtered beside the Town Center (tiles 95..99), not
    // where it grazed eighteen tiles east.
    let killed = killed_at.expect("the sheep was killed");
    assert!(
        killed.x < 101 * FIXED_SUBUNITS_PER_TILE,
        "killed at {killed:?}"
    );
    assert_eq!(food, 100, "every carcass load reached the stockpile");
    assert!(deliveries >= 10, "{deliveries} deliveries");
    assert_eq!(w.unit(worker).unwrap().carried, None);
}

#[test]
fn a_sheep_without_a_food_dropoff_is_slaughtered_where_it_stands() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(118, 97)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(113, 97)).unwrap();
    w.advance();
    let start = w.animal(sheep).unwrap().position;
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    for _ in 0..20 * 30 {
        w.advance();
        assert_ne!(phase(&w, worker, sheep), "herd");
        if w.animal(sheep).is_some_and(|a| a.hp == 0) {
            break;
        }
    }
    let carcass = w.animal(sheep).unwrap();
    assert_eq!(carcass.hp, 0);
    assert_eq!(carcass.position, start);
}

/// On a real map the edge rule refuses a tile against itself, which once froze
/// every animal step that stayed inside its tile: sheep never followed their
/// shepherd and deer never fled.
#[test]
fn animals_walk_on_map_terrain_herded_sheep_follow_and_deer_flee() {
    use crate::game_world::tests::economy::fixture;
    let (package, pages, water) = fixture::flat_fixture(false);
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let (x, y) = (10..60)
        .flat_map(|y| (10..60).map(move |x| (x, y)))
        .find(|&(x, y)| (0..12).all(|dx| w.tile_passable(TileCoord::new(x + dx, y))))
        .expect("an open strip of land");
    w.spawn_villager(PlayerId(0), at(x + 4, y)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(x + 2, y)).unwrap();
    let deer = w.spawn_animal(AnimalKind::Deer, at(x + 6, y)).unwrap();
    w.advance();
    let deer_start = w.animal(deer).unwrap().position;
    w.issue_herd(PlayerId(0), sheep, at(x, y)).unwrap();
    for _ in 0..20 * 5 {
        w.advance();
    }
    assert_eq!(w.animal(sheep).unwrap().position, at(x, y), "herded sheep");
    assert_ne!(w.animal(deer).unwrap().position, deer_start, "deer flee");
}

/// A lead route that never finishes once held the shepherd in Shepherding for
/// ever beside a live sheep, where no client could reassign it (#591). The
/// shepherd here is pinned in place, as a crowd pins it in a real match.
#[test]
fn a_stalled_lead_slaughters_the_sheep_where_the_shepherd_stands() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.place_town_center(PlayerId(0), TileCoord::new(95, 95))
        .unwrap();
    let worker = w.spawn_villager(PlayerId(0), at(118, 97)).unwrap();
    let sheep = w.spawn_animal(AnimalKind::Sheep, at(117, 97)).unwrap();
    w.advance();
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    let mut pinned = None;
    let mut slaughtered = None;
    for tick in 0..20 * 300 {
        w.advance();
        if pinned.is_none() && phase(&w, worker, sheep) == "herd" {
            pinned = Some(w.unit(worker).unwrap().position);
        }
        if let Some(position) = pinned {
            let index = w.lookup[&worker];
            w.units[index].state.position = position;
        }
        if w.animal(sheep).is_some_and(|a| a.hp == 0) {
            slaughtered = Some(tick);
            break;
        }
    }
    assert!(pinned.is_some(), "the shepherd never started leading");
    let tick = slaughtered.expect("the pinned shepherd held its sheep for ever");
    assert!(tick < 20 * 120, "slaughtered only at tick {tick}");
}
