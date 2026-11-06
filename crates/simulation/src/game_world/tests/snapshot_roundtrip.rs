//! Save and resume round trip on a prepared map: the restored game holds the
//! same state, keeps its identities, its workers go back to work, and it plays
//! on.
use super::*;
use crate::game_world::snapshot::{SnapshotError, WorldSnapshot};
use crate::game_world::tests::economy::fixture::flat_fixture;
use aoe_core::TileCoord;
use aoe_map::ResourceKind;

fn flat() -> (GameWorld, TileCoord) {
    let (package, pages, water) = flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let start = (0..8)
        .flat_map(|y| (0..8).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            generator
                .chunk(x, y)
                .unwrap()
                .resources
                .first()
                .map(|n| n.tile)
        })
        .expect("a resource node");
    let world =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    (world, start)
}

/// What a person can see of the game, per entity.
fn view(world: &GameWorld) -> String {
    let mut text = format!("tick {}\n", world.tick().0);
    for player in [PlayerId(0), PlayerId(1)] {
        text += &format!("{:?}\n", world.player_economy(player));
    }
    let mut units: Vec<_> = world.units().collect();
    units.sort_by_key(|unit| unit.id);
    for unit in units {
        text += &format!(
            "{:?} {:?} {:?} {:?} {} {}\n",
            unit.id,
            unit.kind,
            unit.player,
            unit.position,
            unit.hp,
            unit.carried.map_or(0, |c| c.amount)
        );
    }
    let mut buildings: Vec<_> = world.buildings().collect();
    buildings.sort_by_key(|building| building.id);
    for b in buildings {
        text += &format!(
            "{:?} {:?} {:?} {:?} {} {}/{} q{} {:?}\n",
            b.id,
            b.kind,
            b.player,
            b.footprint,
            b.hp,
            b.work_done,
            b.work_total,
            b.training_queue.len(),
            b.research
        );
    }
    text
}

#[test]
fn restored_game_matches_the_saved_one_and_plays_on() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let wood = saved
        .starting_resources()
        .iter()
        .find(|node| node.kind == ResourceKind::Wood)
        .copied()
        .expect("starting wood");
    let villagers: Vec<_> = saved
        .units()
        .filter(|unit| unit.kind == crate::UnitKind::Villager)
        .map(|unit| unit.id)
        .collect();
    assert!(villagers.len() >= 3);
    for id in &villagers[..2] {
        saved.issue_gather(PlayerId(0), *id, wood.id).unwrap();
    }
    let center = saved.buildings().next().unwrap().id;
    saved
        .issue_train(PlayerId(0), center, crate::UnitKind::Villager)
        .unwrap();
    // A Steward sheltered in the Town Center with the owner's settings.
    let at = saved.unit(villagers[2]).unwrap().position;
    let steward = saved
        .spawn_kind(PlayerId(0), crate::UnitKind::Steward, at)
        .unwrap();
    saved.issue_garrison(PlayerId(0), steward, center).unwrap();
    let settings = crate::StewardSettings {
        weights: [1, 5, 2, 0],
        cap: 7,
        training: true,
    };
    saved
        .issue_steward_settings(PlayerId(0), steward, settings)
        .unwrap();
    // Two soldiers on a march with a waypoint behind it.
    let soldiers: Vec<_> = (0..2)
        .map(|i| {
            let tile = TileCoord::new(start.x + 6 + i, start.y + 6);
            saved
                .spawn_kind(
                    PlayerId(0),
                    crate::UnitKind::Militia,
                    aoe_core::WorldPosition::from_tile_center(tile).unwrap(),
                )
                .unwrap()
        })
        .collect();
    let far = aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + 40, start.y + 6))
        .unwrap();
    saved
        .issue_formation_move(
            PlayerId(0),
            &soldiers,
            aoe_core::Formation::default(),
            far,
            false,
            false,
        )
        .unwrap();
    // A queued attack-move behind a plain move keeps its own flag.
    let beyond =
        aoe_core::WorldPosition::from_tile_center(TileCoord::new(start.x + 60, start.y + 6))
            .unwrap();
    saved
        .issue_formation_move(
            PlayerId(0),
            &soldiers,
            aoe_core::Formation::default(),
            beyond,
            true,
            true,
        )
        .unwrap();
    for _ in 0..600 {
        saved.advance();
    }
    let snapshot = saved.snapshot([0; 32]).expect("a map world snapshots");
    assert!(snapshot.units.len() >= villagers.len());

    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    let decoded: WorldSnapshot = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, snapshot);

    let (mut restored, _) = flat();
    restored.economy.limits.animals = 0;
    assert!(
        !snapshot.animals.is_empty(),
        "the village has animals to keep"
    );
    let report = restored
        .restore_snapshot(&decoded, [0; 32], &|| false)
        .unwrap();
    assert_eq!(report.buildings_lost, 0, "{report:?}");
    assert_eq!(report.units_lost, 0, "{report:?}");
    assert_eq!(report.animals_lost, 0, "{report:?}");
    assert_eq!(view(&restored), view(&saved));
    assert_eq!(report.jobs_lost, 0, "{report:?}");
    assert!(report.jobs_resumed >= 2, "{report:?}");
    assert_eq!(restored.next_entity_id, saved.next_entity_id);
    assert_eq!(restored.steward_settings(steward), Some(settings));
    assert!(
        restored.steward_status(steward).is_some(),
        "the Steward is back at work"
    );
    assert!(report.formations >= 1, "{report:?}");
    assert!(report.managers >= 1, "{report:?}");
    assert!(restored.units().any(|u| u.id == soldiers[0] && u.moving));
    let again = restored.snapshot([0; 32]).unwrap();
    assert_eq!(again.explored, snapshot.explored);
    assert_eq!(again.animals, snapshot.animals);
    assert_eq!(again.formations.len(), snapshot.formations.len());
    let queues = |s: &WorldSnapshot| {
        let mut q = s
            .formations
            .iter()
            .map(|f| {
                (
                    f.attack_move,
                    f.queue.iter().map(|w| w.1).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        q.sort();
        q
    };
    assert_eq!(queues(&again), queues(&snapshot));
    assert!(
        queues(&snapshot).iter().any(|(_, q)| q.contains(&true)),
        "the queued attack-move is in the save"
    );

    // The restored game is live: workers gather again, training completes.
    let before = restored.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    for _ in 0..2000 {
        restored.advance();
    }
    assert!(restored.tick().0 > snapshot.tick + 1000);
    assert!(
        restored.player_economy(PlayerId(0)).unwrap().stockpile.wood > before
            || restored.units().count() > villagers.len(),
        "the restored village did nothing in 2000 ticks"
    );
}

#[test]
fn restore_refuses_a_world_that_is_not_empty_or_a_wrong_map() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let snapshot = saved.snapshot([0; 32]).unwrap();
    let (mut busy, start) = flat();
    busy.initialize_village(PlayerId(0), start).unwrap();
    assert!(matches!(
        busy.restore_snapshot(&snapshot, [0; 32], &|| false),
        Err(SnapshotError::NotEmpty)
    ));
    let (mut empty, _) = flat();
    assert!(matches!(
        empty.restore_snapshot(&snapshot, [7; 32], &|| false),
        Err(SnapshotError::Map(_))
    ));
}

/// Saves `saved`, decodes the bytes and restores them into `fresh` (a world
/// built the same way, holding nothing yet).
pub(crate) fn roundtrip(
    saved: &GameWorld,
    mut fresh: GameWorld,
) -> (GameWorld, crate::RestoreReport) {
    let snapshot = saved.snapshot([0; 32]).expect("snapshot");
    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    let decoded: WorldSnapshot = postcard::from_bytes(&bytes).unwrap();
    let report = fresh
        .restore_snapshot(&decoded, [0; 32], &|| false)
        .expect("the snapshot restores");
    (fresh, report)
}

#[test]
fn sheltered_units_and_garrison_orders_in_progress_survive_a_save() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let villagers: Vec<_> = saved
        .units()
        .filter(|u| u.kind == crate::UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    let center = saved.buildings().next().unwrap().id;
    // One villager inside the Town Center, one still walking to it.
    saved
        .issue_garrison(PlayerId(0), villagers[0], center)
        .unwrap();
    for _ in 0..800 {
        saved.advance();
        if saved.building(center).unwrap().garrison.len() == 1 {
            break;
        }
    }
    assert_eq!(saved.building(center).unwrap().garrison.len(), 1);
    saved
        .issue_garrison(PlayerId(0), villagers[1], center)
        .unwrap();
    assert!(saved.economy.garrison_orders.contains_key(&villagers[1]));
    let (restored, report) = roundtrip(&saved, flat().0);
    assert_eq!(
        (report.units_lost, report.buildings_lost),
        (0, 0),
        "{report:?}"
    );
    let building = restored.building(center).unwrap();
    assert_eq!(
        building.garrison.len(),
        1,
        "the sheltered unit stays inside"
    );
    assert_eq!(building.garrison[0].unit.id, villagers[0]);
    assert_eq!(
        restored.economy.garrison_orders.get(&villagers[1]),
        Some(&center),
        "the walk to shelter is ordered again"
    );
}
