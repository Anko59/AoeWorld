use super::manager::{active, pass};
use super::*;
use crate::{VASSAL_RULES, VassalWait};

fn farmers(w: &mut GameWorld, count: i32) -> Vec<EntityId> {
    (0..count)
        .map(|i| {
            let at = TileCoord::new(100 + 4 * (i % 10), 100 + 4 * (i / 10));
            let farm = w
                .insert_building(
                    PlayerId(0),
                    BuildingKind::Farm,
                    aoe_core::TileRect::from_xywh(at.x, at.y, 3, 3),
                    true,
                )
                .unwrap();
            let worker = w
                .spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(at.x + 3, at.y)).unwrap(),
                )
                .unwrap();
            w.issue_farm(PlayerId(0), worker, farm).unwrap();
            worker
        })
        .collect()
}
fn houses(w: &mut GameWorld, count: i32) {
    for i in 0..count {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(3 + (i % 70) * 3, 180 + (i / 70) * 3, 2, 2),
            true,
        )
        .unwrap();
    }
}
fn managed(w: &GameWorld, vassal: EntityId) -> usize {
    w.units()
        .filter(|u| w.vassal_controller(u.id) == Some(vassal))
        .count()
}
fn villagers(w: &GameWorld) -> usize {
    w.units()
        .filter(|u| u.kind == UnitKind::Villager && u.hp > 0)
        .count()
}

/// The owner's report: a Castle placed among 60 busy villagers and two Town
/// Centers with ample food and houses must adopt them all, then keep training
/// to 100 managed villagers, building the Houses it needs on the way.
#[test]
fn a_vassal_adopts_busy_villagers_and_trains_to_a_hundred() {
    let (mut w, _, vassal) = active();
    houses(&mut w, 14);
    w.insert_building(
        PlayerId(0),
        BuildingKind::TownCenter,
        aoe_core::TileRect::from_xywh(110, 70, 4, 4),
        true,
    )
    .unwrap();
    let busy = farmers(&mut w, 60);
    advance(&mut w, 40);
    assert!(busy.iter().all(|id| w.gathering_kind(*id).is_some()));
    pass(&mut w);
    pass(&mut w);
    assert!(
        busy.iter()
            .all(|id| w.vassal_controller(*id) == Some(vassal)),
        "busy farmers are adopted without being retasked"
    );
    assert!(
        busy.iter()
            .all(|id| w.unit(*id).unwrap().task != UnitTask::Idle)
    );
    let status = w.vassal_status(vassal).unwrap();
    assert_eq!(usize::from(status.managed), managed(&w, vassal));
    assert!(status.managed >= 60);
    for _ in 0..30 {
        advance(&mut w, 600);
        if managed(&w, vassal) >= VASSAL_RULES.max_villagers {
            break;
        }
    }
    assert_eq!(managed(&w, vassal), VASSAL_RULES.max_villagers);
    assert_eq!(villagers(&w), 100);
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::House)
            .count()
            > 14,
        "Houses were added for the busy population"
    );
    advance(&mut w, 1_200);
    assert_eq!(villagers(&w), 100, "never beyond the limit");
    assert_eq!(w.vassal_status(vassal).unwrap().wait, VassalWait::Full);
}

#[test]
fn villagers_beyond_the_limit_stay_unmanaged_and_a_manual_move_is_not_adopted() {
    let (mut w, _, vassal) = active();
    let busy = farmers(&mut w, 60);
    let more: Vec<_> = (0..50)
        .map(|i| {
            w.spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(60 + i % 10, 120 + i / 10)).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let walker = more[0];
    w.issue_move(
        walker,
        WorldPosition::from_tile_center(TileCoord::new(60, 150)).unwrap(),
    )
    .unwrap();
    advance(&mut w, 40);
    pass(&mut w);
    assert_eq!(
        w.vassal_controller(walker),
        None,
        "manual order is respected"
    );
    assert!(managed(&w, vassal) <= VASSAL_RULES.max_villagers);
    assert!(busy.iter().any(|id| w.vassal_controller(*id).is_some()));
}

#[test]
fn status_names_what_blocks_growth() {
    let (mut w, _, vassal) = active();
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile::default();
    pass(&mut w);
    assert_eq!(w.vassal_status(vassal).unwrap().wait, VassalWait::Food);
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.stockpile.food = 1_000;
    p.population_used = p.population_cap;
    pass(&mut w);
    assert_eq!(
        w.vassal_status(vassal).unwrap().wait,
        VassalWait::Population
    );
}
