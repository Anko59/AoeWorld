use super::crossing::{banks, river};
use super::*;

#[test]
fn transport_meets_inland_passengers_at_shore_then_sails_to_unload_on_the_other_bank() {
    fn run() -> [u8; 32] {
        let mut world = river();
        let (west, _, east_water, east) = banks(&world);
        let inland = TileCoord::new(west.x - 4, west.y);
        assert!(world.tile_passable(inland));
        let boat = ship(&mut world, UnitKind::TransportShip, east_water, 0);
        let passengers = (0..5)
            .map(|_| world.spawn_militia(PlayerId(0), at(inland)).unwrap())
            .collect::<Vec<_>>();
        world.issue_load(PlayerId(0), &passengers, boat).unwrap();
        assert!(
            world.transport_cargo(boat).is_empty(),
            "boarding requires arrival"
        );
        for _ in 0..800 {
            world.advance();
            // Arrival is individual once physical avoidance separates the group.
            if world.transport_cargo(boat).len() == passengers.len() {
                break;
            }
        }
        assert_eq!(world.transport_cargo(boat), passengers);
        assert_ne!(world.unit(boat).unwrap().position, at(east_water));
        world.issue_unload(PlayerId(0), boat, east).unwrap();
        for _ in 0..800 {
            world.advance();
            if world.transport_cargo(boat).is_empty() {
                break;
            }
        }
        assert!(world.transport_cargo(boat).is_empty());
        for passenger in passengers {
            // Ordinary soft spacing separates a full load within the landing tile.
            assert_eq!(world.unit(passenger).unwrap().position.tile_floor(), east);
            assert!(!world.embarked(passenger));
            assert!(world.unit_visible_to(PlayerId(0), passenger));
        }
        world.canonical_hash()
    }
    assert_eq!(run(), run(), "shore rendezvous replays deterministically");
}

#[test]
fn a_new_move_or_stop_cancels_pending_boarding() {
    for stop_ship in [false, true] {
        let mut world = river();
        let (west, _, east_water, _) = banks(&world);
        let inland = TileCoord::new(west.x - 4, west.y);
        let boat = ship(&mut world, UnitKind::TransportShip, east_water, 0);
        let passenger = world.spawn_militia(PlayerId(0), at(inland)).unwrap();
        world.issue_load(PlayerId(0), &[passenger], boat).unwrap();
        if stop_ship {
            world.issue_stop(PlayerId(0), boat).unwrap();
        } else {
            world.issue_move(passenger, at(inland)).unwrap();
        }
        advance(&mut world, 800);
        assert!(world.transport_cargo(boat).is_empty());
        assert!(world.unit_visible_to(PlayerId(0), passenger));
        assert!(world.economy.transport_journeys.is_empty());
    }
}

#[test]
fn unloading_on_arrival_reports_the_ship_and_landed_passenger_in_the_tick() {
    let mut world = river();
    let (west, water, _, east) = banks(&world);
    let boat = ship(&mut world, UnitKind::TransportShip, water, 0);
    let passenger = world.spawn_militia(PlayerId(0), at(west)).unwrap();
    world.issue_load(PlayerId(0), &[passenger], boat).unwrap();
    world.issue_unload(PlayerId(0), boat, east).unwrap();
    for _ in 0..800 {
        let changed = world.advance();
        if world.transport_cargo(boat).is_empty() {
            assert!(changed.iter().any(|u| u.id == boat));
            assert!(changed.iter().any(|u| u.id == passenger));
            return;
        }
    }
    panic!("ship never unloaded at the other shore");
}

#[test]
fn boarding_chooses_a_shore_reachable_by_the_ship_instead_of_the_nearest_disconnected_stream() {
    let mut world = super::crossing::river_channels(10 * 256, true);
    let (land, water, main_bank) = (30..210)
        .find_map(|y| {
            let Terrain::Map { generator, .. } = world.terrain() else {
                unreachable!()
            };
            let wet = |x| {
                generator
                    .tile_at(TileCoord::new(x, y))
                    .is_some_and(|t| t.water == aoe_map::WaterKind::River)
            };
            let small_end = (110..121).filter(|x| wet(*x)).max()?;
            let main_start = (121..136).find(|x| wet(*x))?;
            let land = TileCoord::new(small_end + 1, y);
            (main_start - small_end >= 4
                && world.tile_passable(land)
                && world
                    .unit_tile_passable(UnitKind::TransportShip, TileCoord::new(128, y), &|| false)
                    .ok()?)
            .then_some((land, TileCoord::new(128, y), main_start))
        })
        .expect("two disconnected streams with a clear dry strip");
    let boat = ship(&mut world, UnitKind::TransportShip, water, 0);
    let passenger = world.spawn_militia(PlayerId(0), at(land)).unwrap();
    world.issue_load(PlayerId(0), &[passenger], boat).unwrap();
    for _ in 0..800 {
        world.advance();
        if !world.transport_cargo(boat).is_empty() {
            break;
        }
    }
    assert_eq!(world.transport_cargo(boat), &[passenger]);
    assert!(world.unit(boat).unwrap().position.tile_floor().x >= main_bank - 2);
}
