//! A remembered building that is destroyed leaves memory by sight, not by luck.
use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

fn remembers(w: &GameWorld, player: PlayerId, id: EntityId) -> bool {
    w.known_buildings(player).iter().any(|b| b.id == id)
}

/// Player 0 sees player 1's town center, then loses sight of it.
fn remembered_then_unwatched() -> (GameWorld, EntityId) {
    let mut w = world();
    let scout = w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    let id = w
        .place_town_center(PlayerId(1), TileCoord::new(65, 65))
        .unwrap();
    w.advance();
    assert!(remembers(&w, PlayerId(0), id));
    w.remove_unit(scout);
    w.advance();
    assert!(
        remembers(&w, PlayerId(0), id),
        "fogged buildings stay remembered"
    );
    (w, id)
}

#[test]
fn a_ghost_stays_while_fogged_and_vanishes_when_the_empty_site_is_seen() {
    let (mut w, id) = remembered_then_unwatched();
    w.remove_building(id);
    w.advance();
    assert!(
        remembers(&w, PlayerId(0), id),
        "destroyed unseen: last sighting stays"
    );
    w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    w.advance();
    assert!(
        !remembers(&w, PlayerId(0), id),
        "the empty site was revisited"
    );
}

#[test]
fn a_building_destroyed_in_view_disappears_at_once() {
    let mut w = world();
    w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    let id = w
        .place_town_center(PlayerId(1), TileCoord::new(65, 65))
        .unwrap();
    w.advance();
    assert!(remembers(&w, PlayerId(0), id));
    w.remove_building(id);
    assert!(!remembers(&w, PlayerId(0), id));
}

#[test]
fn the_owner_forgets_its_destroyed_building_immediately() {
    let (mut w, id) = remembered_then_unwatched();
    w.remove_building(id);
    assert!(!remembers(&w, PlayerId(1), id));
    w.advance();
    assert!(!remembers(&w, PlayerId(1), id));
}

#[test]
fn a_ghost_survives_a_fog_cycle_until_its_site_is_seen() {
    let (mut w, id) = remembered_then_unwatched();
    w.remove_building(id);
    w.set_fog_disabled(true);
    w.set_fog_disabled(false);
    w.advance();
    assert!(
        remembers(&w, PlayerId(0), id),
        "fog toggling is not a sighting"
    );
    w.spawn_unit(PlayerId(0), at(65, 65)).unwrap();
    w.advance();
    assert!(!remembers(&w, PlayerId(0), id));
}

#[test]
fn a_building_destroyed_while_fog_is_off_leaves_a_ghost_for_other_players() {
    let (mut w, id) = remembered_then_unwatched();
    w.set_fog_disabled(true);
    w.remove_building(id);
    w.set_fog_disabled(false);
    w.advance();
    assert!(remembers(&w, PlayerId(0), id));
    assert!(!remembers(&w, PlayerId(1), id));
    w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    w.advance();
    assert!(!remembers(&w, PlayerId(0), id));
}

#[test]
fn re_enabling_fog_does_not_keep_players_who_lost_sight_as_watchers() {
    let (mut w, id) = remembered_then_unwatched();
    let before = w.known_buildings(PlayerId(0));
    w.set_fog_disabled(true);
    w.set_fog_disabled(false);
    w.economy.buildings.get_mut(&id).unwrap().hp -= 100;
    w.advance();
    let after = w.known_buildings(PlayerId(0));
    assert_eq!(after[0].hp, before[0].hp, "unseen damage stays unseen");
}
