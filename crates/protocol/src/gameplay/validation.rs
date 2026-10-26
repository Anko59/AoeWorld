use super::*;

pub(super) fn validate_server(message: &ServerMessage) -> Result<(), Error> {
    queues::validate(message)?;
    wildlife::validate_server(message)?;
    if let ServerMessage::Projectiles {
        revision,
        tick,
        projectiles,
    } = message
        && (*revision == 0 || !projectiles::valid(projectiles, *tick))
    {
        return Err(Error::InvalidEconomyState);
    }
    if let ServerMessage::FogState(state) = message
        && !state.valid()
    {
        return Err(Error::InvalidEconomyState);
    }
    match message {
        ServerMessage::ResourceState(state) if !state.valid() => Err(Error::InvalidResourceState),
        ServerMessage::Snapshot {
            units,
            players,
            buildings,
            ..
        } => {
            if units.len() > MAX_SUBSCRIBED_UNITS {
                return Err(Error::TooManyUnits);
            }
            if !economy::valid_units(units)
                || !economy::valid_players(players)
                || !economy::valid_buildings(buildings)
            {
                return Err(Error::InvalidEconomyState);
            }
            Ok(())
        }
        ServerMessage::Tick {
            changed_units,
            removals,
            changed_players,
            changed_buildings,
            building_removals,
            ..
        } => {
            if changed_units.len() > MAX_SUBSCRIBED_UNITS || removals.len() > MAX_SUBSCRIBED_UNITS {
                return Err(Error::TooManyUnits);
            }
            if !economy::valid_units(changed_units)
                || !economy::valid_players(changed_players)
                || !economy::valid_buildings(changed_buildings)
                || building_removals.len() > MAX_ECONOMY_BUILDINGS
            {
                return Err(Error::InvalidEconomyState);
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
