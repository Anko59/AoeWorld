use super::*;
use aoe_core::{EntityId, PlayerId, Tick};
use aoe_protocol::{GameplayRole, GameplayServerMessage, GameplayUnitState, UnitTask};

fn unit(id: u32, kind: UnitKind, tile: TileCoord) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(id),
        player: PlayerId(0),
        position: aoe_core::WorldPosition::from_tile_center(tile).unwrap(),
        moving: false,
        planning: false,
        facing: 0,
        kind,
        task: UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 30,
        max_hp: 30,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
    }
}

fn world() -> World {
    let welcome = GameplayServerMessage::Welcome {
        version: aoe_protocol::GAMEPLAY_VERSION,
        world_id: 1,
        map_content_hash: None,
        map_metadata: None,
        width_tiles: 900,
        height_tiles: 900,
        coordinate_precision: 1024,
        tick_hz: 20,
        role: GameplayRole::Controller,
        primary_unit_id: EntityId(1),
        resume_token: None,
    };
    let mut world = World::from_welcome(&welcome).unwrap();
    world.me = Some(PlayerId(0));
    world.units.insert(
        EntityId(1),
        unit(1, UnitKind::Villager, TileCoord::new(100, 100)),
    );
    world
}

#[test]
fn a_capped_follow_window_keeps_the_army_in_sight_instead_of_empty_ground() {
    let mut world = world();
    let front = TileCoord::new(500, 500);
    for id in 2..14 {
        world
            .units
            .insert(EntityId(id), unit(id, UnitKind::Spearman, front));
    }
    // Reinforcements at home do not pull the view halfway back across the map.
    world.units.insert(
        EntityId(14),
        unit(14, UnitKind::Archer, TileCoord::new(100, 100)),
    );
    let region = desired_region(&world, ViewPolicy::default()).unwrap();
    assert!(region.contains(front), "army is outside {region:?}");
    assert!(region.width() <= 128 && region.height() <= 128);
}

#[test]
fn a_remote_scout_or_dead_soldier_does_not_drag_the_economy_view_away() {
    let mut world = world();
    world.units.insert(
        EntityId(2),
        unit(2, UnitKind::Scout, TileCoord::new(500, 500)),
    );
    let mut dead = unit(3, UnitKind::Spearman, TileCoord::new(500, 500));
    dead.hp = 0;
    dead.death_tick = Some(Tick(1));
    world.units.insert(dead.id, dead);
    let region = desired_region(&world, ViewPolicy::default()).unwrap();
    assert!(
        region.contains(TileCoord::new(100, 100)),
        "home is outside {region:?}"
    );
}
