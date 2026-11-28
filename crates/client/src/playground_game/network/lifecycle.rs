//! Persistent connection diagnostics and world replacement.
use super::*;

pub(super) fn set_controller_ready(client: &Client, ready: bool) {
    if let Some(playground) = client.document.get_element_by_id("playground") {
        let _ = playground.set_attribute(
            "data-controller-ready",
            if ready { "true" } else { "false" },
        );
    }
}

/// Forgets the resume token: the next connection is a new controller.
pub(super) fn forget_session(client: &mut Client) {
    client.token = None;
    storage::save_token(None);
    set_controller_ready(client, false);
}

pub(super) fn record_close(client: &mut Client, event: &Event) {
    set_controller_ready(client, false);
    village::construction::strategic::connection_reset(client);
    let reason = js_sys::Reflect::get(&event, &JsValue::from_str("reason"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "The connection to the server was lost.".to_owned());
    match &mut client.last_error {
        Some(error) if !error.contains(&reason) => {
            error.push(' ');
            error.push_str(&reason);
        }
        Some(_) => {}
        None => client.last_error = Some(reason),
    }
}

pub(super) fn reconnect(client: &mut Client, status: &str) {
    village::construction::strategic::connection_reset(client);
    client.status = status.to_owned();
    client.last_error = Some(status.to_owned());
    client.resources.clear();
    if let Some(socket) = &client.socket {
        let _ = socket.close();
    }
}

// The pieces of a connection's world that a new Welcome or a WorldReset drops.
pub(super) fn clear_world(client: &mut Client) {
    client.capture.current_view = false;
    client.capture.snapshot_revision = None;
    client.terrain_scene.get_mut().take();
    client.resources.clear();
    client.fog.clear();
    if !client.surface_fixture {
        map::clear_terrain_cache(client);
    }
    client.units.clear();
    client.village = Default::default();
    village::diplomacy::clear();
    client.selected = Default::default();
    let tick_hz = client.config.tick_hz;
    client.presentation.reset(tick_hz);
    client.rendered_frame.clear();
    capture::update(client);
}

/// What removed buildings leave, from their last known state.
pub(super) fn leave_rubble(client: &mut Client, removals: &[EntityId], tick: u64) {
    for id in removals {
        let retained = &client.village.buildings;
        if let Ok(index) = retained.binary_search_by_key(id, |b| b.id) {
            let b = &retained[index];
            if client.village.rubble.len() < aoe_protocol::MAX_ECONOMY_BUILDINGS
                && (b.kind != aoe_protocol::BuildingKind::Farm
                    || b.food_remaining.is_some_and(|food| food > 0))
            {
                let life = map::scene::rubble_ticks(client, b);
                let b = b.clone();
                client.village.rubble.push((b, tick.saturating_add(life)));
            }
        }
    }
}

/// Remembers when each shooter last released, so its animation can be phased
/// to the shot. Entries of idle shooters expire after half a minute.
pub(super) fn record_shots(
    village: &mut village::Village,
    projectiles: &[aoe_protocol::ProjectileState],
    tick: u64,
    hz: u32,
) {
    let particles = &mut village.projectile_particles;
    particles.retain(|_, shot| tick.saturating_sub(shot.impact.0) < u64::from(hz) * 2);
    for shot in projectiles {
        if let Some(attacker) = shot.attacker {
            let last = village.last_shot.entry(attacker).or_insert(shot.launched.0);
            *last = (*last).max(shot.launched.0);
        }
        if let aoe_protocol::ProjectileKind::Native(id) = shot.kind {
            let (muzzle, impact) = aoe_assets::catalog::units::projectile_effects(id);
            if muzzle.is_some() || impact.is_some() {
                particles.insert(shot.id, *shot);
            }
        }
    }
    village
        .last_shot
        .retain(|_, launched| tick.saturating_sub(*launched) < 600);
}

pub(in super::super) fn merge_buildings(
    retained: &[aoe_protocol::GameplayBuildingState],
    changed: &[aoe_protocol::GameplayBuildingState],
    removals: &[aoe_core::EntityId],
) -> Option<Vec<aoe_protocol::GameplayBuildingState>> {
    // All three wire lists are ordered by ID. Walk them once without building
    // a second map of the entire kingdom for each group of changed sites.
    let mut retained = retained.iter().peekable();
    let mut changed = changed.iter().peekable();
    let mut removals = removals.iter().copied().peekable();
    let mut merged = Vec::with_capacity(retained.len().max(changed.len()));
    while let Some(id) = retained
        .peek()
        .map(|b| b.id)
        .into_iter()
        .chain(changed.peek().map(|b| b.id))
        .min()
    {
        let old = retained.next_if(|b| b.id == id);
        while removals.next_if(|removed| *removed < id).is_some() {}
        let removed = removals.next_if_eq(&id).is_some();
        if let Some(building) = changed.next_if(|b| b.id == id).or(old.filter(|_| !removed)) {
            if merged.len() == aoe_protocol::MAX_RETAINED_BUILDINGS {
                return None;
            }
            merged.push(building.clone());
        }
    }
    Some(merged)
}
