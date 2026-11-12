use super::*;
use aoe_core::{Seed, WorldConfig};

fn scene(friends: usize) -> (GameWorld, EntityId, EntityId, TargetArea) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let tile = SPATIAL_CHUNK_TILES + 4;
    let at = WorldPosition::new(
        tile * FIXED_SUBUNITS_PER_TILE,
        tile * FIXED_SUBUNITS_PER_TILE,
    );
    let actor = world.spawn_unit(PlayerId(0), at).unwrap();
    for _ in 0..friends {
        world.spawn_unit(PlayerId(0), at).unwrap();
    }
    let enemy = world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::new(at.x + 3 * FIXED_SUBUNITS_PER_TILE, at.y),
        )
        .unwrap();
    let source = world.unit(actor).unwrap();
    let area = TargetArea {
        player: source.player,
        source: Goal::Unit(source.position, source.kind.collision_radius()),
        min: 0,
        max: i64::from(FIXED_SUBUNITS_PER_TILE),
        actor,
        ram: false,
    };
    (world, actor, enemy, area)
}

#[test]
fn a_sampled_enemy_rejects_quiet_even_outside_the_attack_window() {
    let (mut world, _, enemy, area) = scene(0);
    assert!(
        !area
            .bounds(&world)
            .contains(world.unit(enemy).unwrap().position.tile_floor())
    );
    let result = world.acquire_target(&area, true);
    assert!(result.target.is_none());
    assert!(result.sampled_hostile);
}

#[test]
fn a_retained_corpse_does_not_report_a_sampled_hostile() {
    let (mut world, _, enemy, area) = scene(0);
    world.issue_delete(PlayerId(1), &[enemy]).unwrap();
    assert_eq!(world.unit(enemy).unwrap().hp, 0);
    assert!(
        world
            .chunks
            .get(&ChunkCoord { x: 1, y: 1 })
            .unwrap()
            .contains(&enemy)
    );
    let result = world.acquire_target(&area, true);
    assert!(result.target.is_none());
    assert!(!result.sampled_hostile);
}

#[test]
fn an_unsampled_enemy_still_requires_the_full_quiet_proof() {
    let (mut world, actor, _, area) = scene(BUCKET_WINDOW * 2);
    let result = world.acquire_target(&area, true);
    assert!(result.target.is_none());
    assert!(!result.sampled_hostile);
    assert_eq!(world.combat_stats().targeting_scanned, BUCKET_WINDOW as u32);
    let source = world.unit(actor).unwrap();
    world.sleep_quiet_acquisition(&source, &area);
    assert!(!world.skip_quiet_acquisition(actor));
}
