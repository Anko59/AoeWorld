use super::{Client, map, set_text};

pub(super) fn update(client: &Client) {
    set_text(&client.document, "connection", &client.status);
    set_text(&client.document, "unit-state", &label(client));
    set_text(
        &client.document,
        "terrain-cache",
        &map::cache_status(client),
    );
    set_text(
        &client.document,
        "tile-inspection",
        &map::inspection_label(client),
    );
}

pub(super) fn label(client: &Client) -> String {
    client
        .selected
        .and_then(|id| client.units.get(&id))
        .map(|unit| movement_label(unit.moving, unit.planning))
        .map_or_else(
            || "no unit selected".to_owned(),
            |movement| format!("unit {movement}"),
        )
}

const fn movement_label(moving: bool, planning: bool) -> &'static str {
    if planning {
        "planning"
    } else if moving {
        "moving"
    } else {
        "idle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_unit_state_is_explicit_in_its_own_readout() {
        assert_eq!(movement_label(false, true), "planning");
        assert_eq!(movement_label(true, false), "moving");
        assert_eq!(movement_label(false, false), "idle");
    }
}
