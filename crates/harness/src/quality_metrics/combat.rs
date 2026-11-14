use super::{
    Result,
    march::{self, OrderPath, at},
};
use aoe_core::{PlayerId, Seed, WorldConfig};
use aoe_simulation::{GameWorld, UnitKind};
use serde_json::Value;

pub(super) fn measure(
    name: &str,
    path: OrderPath,
    video: Option<&std::path::Path>,
) -> Result<Value> {
    let world = GameWorld::new(WorldConfig::new(256, 256, Seed(0))?)?;
    let kind = if name == "ranged" {
        UnitKind::Archer
    } else {
        UnitKind::Militia
    };
    let mut scene = march::populate(world, (35, 100), (140, 100), path, kind, 2400)?;
    scene.attack_move = true;
    for n in 0..scene.ids.len() as i32 {
        scene
            .world
            .spawn_kind(PlayerId(1), kind, at(125 + n % 5, 100 + n / 5)?)?;
    }
    if name == "attack-move-defended" {
        aoe_simulation::terrain_fixtures::measurement_tower(
            &mut scene.world,
            aoe_core::TileCoord::new(130, 95),
        );
    }
    scene.measure(name, video)
}
