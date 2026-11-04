use super::manager::{active, pass};
use super::*;
use crate::{STEWARD_RULES, StewardWait};

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
fn managed(w: &GameWorld, steward: EntityId) -> usize {
    w.units()
        .filter(|u| w.steward_controller(u.id) == Some(steward))
        .count()
}
fn villagers(w: &GameWorld) -> usize {
    w.units()
        .filter(|u| u.kind == UnitKind::Villager && u.hp > 0)
        .count()
}

/// The owner's report: a Town Center placed among 60 busy villagers and two Town
/// Centers with ample food must adopt busy villagers, then keep training to
/// 100 managed villagers with enough housing.
#[test]
fn a_steward_adopts_busy_villagers_and_trains_to_a_hundred() {
    let (mut w, _, steward) = active();
    for i in 0..20 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(10 + 3 * (i % 10), 10 + 3 * (i / 10), 2, 2),
            true,
        )
        .unwrap();
    }
    super::manager::training_on(&mut w, steward);
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
            .all(|id| w.steward_controller(*id) == Some(steward)),
        "busy farmers are adopted without being retasked"
    );
    assert!(
        busy.iter()
            .all(|id| w.unit(*id).unwrap().task != UnitTask::Idle)
    );
    let status = w.steward_status(steward).unwrap();
    assert_eq!(usize::from(status.managed), managed(&w, steward));
    assert!(status.managed >= 60);
    for _ in 0..30 {
        advance(&mut w, 600);
        if managed(&w, steward) >= STEWARD_RULES.max_villagers {
            break;
        }
    }
    assert_eq!(
        managed(&w, steward),
        STEWARD_RULES.max_villagers,
        "unmanaged villagers (id, task, gather kind/tile, movement, controller): {:?}",
        w.units()
            .filter(|u| u.kind == UnitKind::Villager && w.steward_controller(u.id) != Some(steward))
            .map(|u| (
                u.id,
                u.task,
                w.gathering_kind(u.id),
                w.gather_tile(u.id),
                w.movement_order(u.id),
                w.steward_controller(u.id)
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(villagers(&w), 100);
    advance(&mut w, 1_200);
    assert_eq!(villagers(&w), 100, "never beyond the limit");
    assert_eq!(w.steward_status(steward).unwrap().wait, StewardWait::Full);
}

#[test]
fn villagers_beyond_the_limit_stay_unmanaged_and_a_manual_move_is_not_adopted() {
    let (mut w, _, steward) = active();
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
        w.steward_controller(walker),
        None,
        "manual order is respected"
    );
    assert!(managed(&w, steward) <= STEWARD_RULES.max_villagers);
    assert!(busy.iter().any(|id| w.steward_controller(*id).is_some()));
}

#[test]
fn status_names_what_blocks_growth() {
    let (mut w, _, steward) = active();
    super::manager::training_on(&mut w, steward);
    w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile = Stockpile::default();
    pass(&mut w);
    assert_eq!(w.steward_status(steward).unwrap().wait, StewardWait::Food);
}
