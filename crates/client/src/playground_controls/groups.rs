//! Local groups retain positions when their units leave the camera subscription.
use super::{Client, navigation};
use aoe_core::EntityId;

#[inline(never)]
fn positions(client: &Client) -> Vec<f64> {
    let owner = crate::playground::game::village::owner(client);
    let mut values = Vec::new();
    for u in client.units.values() {
        let p = if u.player == owner && u.hp > 0 {
            u.position.as_tiles()
        } else {
            [f64::NAN; 2]
        };
        values.extend_from_slice(&[f64::from(u.id.0), p[0], p[1]]);
    }
    values
}

pub(in super::super) fn update(client: &Client) {
    if navigation::group_members() != 0 {
        navigation::groups_update(&positions(client));
    }
}

#[inline(never)]
pub(super) fn key(client: &mut Client, action: u32) {
    let selected: Vec<_> = client.selected.units.iter().map(|id| id.0).collect();
    let result = navigation::control_group(action, &selected, &positions(client)).to_vec();
    if result.is_empty() {
        return;
    }
    crate::playground::game::village::construction::cancel(client);
    client.village.move_cursor = false;
    client.village.unload_cursor = false;
    client.village.attack_move = false;
    client
        .selected
        .units(result.into_iter().map(EntityId).collect());
}
