//! Where a route planner is stored. A planner is 944 bytes (inline search
//! states, ordered collections, a queue and vectors) and at most
//! `MAX_ACTIVE_ROUTE_PLANNERS` are alive, while every pass over the units
//! strides over every unit record. Held inline it made each record 1,264
//! bytes, nearly always around an empty slot, and every poll copied the
//! planner out of its record and back.
use super::*;
use crate::game_movement::MAX_ACTIVE_ROUTE_PLANNERS;
use std::mem::{size_of, size_of_val};

/// Bytes one unit record may take: the 328 it needs with its planner behind
/// a pointer, rounded up to the next 64.
const MAX_UNIT_RECORD_BYTES: usize = 384;

#[test]
fn a_unit_record_does_not_hold_its_route_planner_inline() {
    let record = size_of::<StoredUnit>();
    assert!(
        record <= MAX_UNIT_RECORD_BYTES,
        "a unit record takes {record} bytes, over the {MAX_UNIT_RECORD_BYTES} allowed: a route \
         planner is {} bytes and at most {MAX_ACTIVE_ROUTE_PLANNERS} are alive, so it belongs \
         behind a pointer",
        size_of::<RoutePlanner>()
    );
}

/// `take_planner` and `store_planner` run around every poll. What changes
/// hands there is the pointer: a planner moved out of its allocation and
/// allocated again on the way back would cost each poll both copies and an
/// allocation. The sizes are those of the types, so the unit needs no planner.
#[test]
fn a_route_planner_changes_hands_as_one_pointer() {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    let at = WorldPosition::from_tile_center(TileCoord::new(10, 10)).expect("tile");
    let id = world.spawn_unit(PlayerId(0), at).expect("unit");
    let index = world.lookup[&id];
    let slot = size_of_val(&world.units[index].planner);
    let taken = world.take_planner(index);
    let handed = size_of_val(&taken);
    world.store_planner(index, taken);
    let pointer = size_of::<usize>();
    assert!(
        slot <= pointer && handed <= pointer,
        "a unit's planner slot is {slot} bytes and taking it hands over {handed}: each is one \
         pointer ({pointer} bytes) once the planner is boxed"
    );
}
