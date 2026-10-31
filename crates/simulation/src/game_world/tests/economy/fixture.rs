pub(crate) use crate::game_world::terrain_fixtures::flat_fixture;

/// Survey legal build sites without leaving an observer that could mask
/// construction sight. Economy fixtures still place through ordinary orders.
pub(crate) fn survey(
    world: &mut crate::GameWorld,
    player: aoe_core::PlayerId,
    center: aoe_core::TileCoord,
    radius: u8,
) {
    // Reserve an ordinary transient ID: observer storage is dense by the
    // sequential entity allocator, so a sentinel ID would allocate 64 GiB.
    let observer = world.allocate_entity_id().expect("survey observer id");
    world.update_observer(observer, Some((player, center, radius)));
    world.update_observer(observer, None);
}
