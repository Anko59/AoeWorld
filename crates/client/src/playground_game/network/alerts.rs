//! Tells the minimap what struck the viewer's own units and buildings, from one
//! tick and before it is applied: the retained state is what was known.
use super::*;
use crate::alerts::{answered, building_hit, building_lost, hunting, replaced, unit_hit};
use aoe_protocol::{GameplayBuildingState, HitState};

/// Remembers what the player removes himself: its loss is no alert.
pub(in super::super) fn sent(client: &mut Client, message: &GameplayClientMessage) {
    let removed: &[EntityId] = match message {
        GameplayClientMessage::DeleteOrder { entity_ids, .. } => entity_ids,
        GameplayClientMessage::CancelConstructionOrder { building_id, .. } => {
            std::slice::from_ref(building_id)
        }
        _ => return,
    };
    let tick = client.village.tick;
    let tick_hz = client.config.tick_hz;
    let deleted = &mut client.village.deleted;
    deleted.retain(|_, sent| answered(*sent, tick, tick_hz));
    deleted.extend(removed.iter().map(|id| (*id, tick)));
}

/// Work is proportional to the changes of the tick, never to the kingdom.
pub(super) fn tell(
    client: &Client,
    tick: u64,
    units: &[GameplayUnitState],
    buildings: &[GameplayBuildingState],
    lost: &[EntityId],
) {
    let owner = village::owner(client);
    let tick_hz = client.config.tick_hz;
    let building = |id: EntityId| {
        let known = &client.village.buildings;
        let index = known.binary_search_by_key(&id, |b| b.id).ok()?;
        known.get(index)
    };
    // By the viewer's own side, or by game. A striker the client never heard of
    // is hidden or off the camera: an enemy, or a wolf.
    let harmless = |hit: &HitState| {
        let id = hit.attacker;
        let unit = client.units.get(&id).map(|unit| unit.player);
        if let Some(player) = unit.or_else(|| building(id).map(|b| b.player)) {
            return player == owner;
        }
        client.village.game.contains(&id)
    };
    // A unit carries its last hit for life: an old one needs no lookup.
    let recent = |hit: HitState| tick.saturating_sub(hit.tick.0) <= u64::from(tick_hz);
    let deleted = |id: &EntityId| {
        let order = client.village.deleted.get(id);
        order.is_some_and(|sent| answered(*sent, tick, tick_hz))
    };
    let mut places: Vec<[f64; 2]> = Vec::new();
    places.extend(
        units
            .iter()
            .filter(|unit| unit.player == owner && unit.last_hit.is_some_and(recent))
            .filter_map(|unit| {
                let hunted =
                    |hit: &HitState| hunting(unit) && unit.hunt_target == Some(hit.attacker);
                let known = client.units.get(&unit.id);
                unit_hit(known, unit, |hit| hunted(hit) || harmless(hit))
            }),
    );
    places.extend(
        buildings
            .iter()
            .filter(|told| told.player == owner && told.last_hit.is_some())
            .filter_map(|told| building_hit(building(told.id), told, harmless)),
    );
    places.extend(
        lost.iter()
            .filter(|id| !deleted(id))
            .filter_map(|id| building(*id))
            .filter(|known| known.player == owner)
            .filter_map(|known| {
                let place = building_lost(known, tick, tick_hz, harmless)?;
                (!replaced(known, buildings)).then_some(place)
            }),
    );
    if places.is_empty() {
        return;
    }
    // A covered scene shows no fight, wherever its camera stands.
    if !strategic::active(client) && !controls::navigation::overview_active() {
        places.retain(|place| !on_screen(client, *place));
    }
    if !places.is_empty() {
        controls::navigation::navigation_hits(places.as_flattened());
    }
}

fn on_screen(client: &Client, place: [f64; 2]) -> bool {
    let [x, y] = place.map(|value| value.floor() as i32);
    // Land that is not loaded has no height: the camera's own plane is nearer
    // to it than sea level.
    let point = if map::terrain_tile(client, x, y).is_some() {
        let height = map::elevation_at_world(client, place);
        client.camera.world_to_screen_at_height(place, height)
    } else {
        strategic::screen_position(client, place)
    };
    let [width, height] = client.camera.viewport;
    (0.0..width).contains(&point.x) && (0.0..height).contains(&point.y)
}
