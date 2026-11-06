//! A Vassal on typed hydrology: its road to the far-bank Town Center spans the
//! river with a bridge that its builders raise and then cross.
use super::*;
use crate::game_world::terrain_fixtures::build;

#[test]
fn a_vassal_bridges_a_real_river_and_its_builders_cross() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let (v, span) = world.prepare_vassal_river_demo().unwrap();
    let count = |w: &GameWorld| {
        w.buildings()
            .filter(|b| b.kind == BuildingKind::Bridge)
            .count()
    };
    let standing = count(world);
    let along_x = span.from.x != span.to.x;
    let beyond = |t: TileCoord| {
        if along_x {
            t.x > span.to.x
        } else {
            t.y > span.to.y
        }
    };
    let (mut built, mut crossed) = (None, None);
    for t in 0..60_000_u32 {
        world.advance();
        if built.is_none()
            && world
                .buildings()
                .filter(|b| b.kind == BuildingKind::Bridge && b.is_complete())
                .count()
                > standing
        {
            built = Some(t);
        }
        if built.is_some()
            && world
                .units()
                .any(|u| u.kind == UnitKind::Villager && beyond(u.position.tile_floor()))
        {
            crossed = Some(t);
            break;
        }
    }
    eprintln!(
        "real river {span:?}: bridge complete at {built:?}, a builder beyond it at {crossed:?}; {:?}",
        world.vassal_status(v)
    );
    assert!(built.is_some(), "{:?}", world.vassal_status(v));
    assert!(crossed.is_some(), "builders must cross the bridge");
    // Re-planning keeps the span it laid: no parallel bridges.
    for _ in 0..6_000 {
        world.advance();
    }
    assert_eq!(count(world) - standing, 1, "one new bridge over the river");
}
