use super::Client;

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
