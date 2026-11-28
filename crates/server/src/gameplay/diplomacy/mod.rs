use super::{Session, Slot};
use aoe_core::PlayerId;
use aoe_protocol::{
    AllianceOffer, AlliedMarket, DiplomacySeat, DiplomacyView, GameplayServerMessage,
};
use aoe_simulation::{DiplomaticStance, GameWorld};
use std::collections::BTreeSet;

#[cfg(test)]
mod tests;

pub(super) fn view(
    world: &GameWorld,
    slots: &[Slot],
    viewer: PlayerId,
    revision: u64,
    ai: bool,
) -> Option<DiplomacyView> {
    if slots.len() < 2 {
        return None;
    }
    let seat = |player, name| DiplomacySeat {
        player,
        name,
        stance: world.diplomatic_stance(viewer, player),
        offer: if world.offered_alliance(player, viewer) && (!ai || !world.at_war(player, viewer)) {
            AllianceOffer::Received
        } else if !ai && world.offered_alliance(viewer, player) {
            AllianceOffer::Sent
        } else {
            AllianceOffer::None
        },
    };
    let mut seats = if ai {
        // The AI needs only its non-default relationships and proposals.
        world
            .diplomatic_overrides(viewer)
            .filter(|&(_, stance)| stance != DiplomaticStance::Enemy)
            .map(|(player, _)| player)
            .chain(world.alliance_requests(viewer))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|player| seat(player, None))
            .filter(|entry| {
                entry.stance != DiplomaticStance::Enemy || entry.offer != AllianceOffer::None
            })
            .collect::<Vec<_>>()
    } else {
        slots
            .iter()
            .map(|slot| seat(slot.player, slot.name.clone()))
            .collect::<Vec<_>>()
    };
    if seats.is_empty() {
        return None;
    }
    seats.sort_unstable_by_key(|seat| seat.player);
    let mut markets: Vec<_> = world
        .allied_market_positions(viewer)
        .map(|(player, tile)| AlliedMarket { player, tile })
        .collect();
    markets.sort_unstable();
    markets.dedup();
    Some(DiplomacyView {
        revision,
        viewer,
        seats,
        markets,
        tribute_market: (!ai).then(|| world.tribute_market(viewer)).flatten(),
        tribute_fee_permyriad: if ai {
            0
        } else {
            aoe_simulation::tribute_fee_permyriad(
                world.player_economy(viewer).map_or(0, |p| p.researched),
            )
        },
    })
}

pub(super) fn message(
    session: &Session,
    revision: u64,
    view: Option<&DiplomacyView>,
) -> Option<GameplayServerMessage> {
    if session.resident_diplomacy.as_ref() == view {
        return None;
    }
    Some(GameplayServerMessage::Diplomacy(view.cloned().unwrap_or(
        DiplomacyView {
            revision,
            viewer: session.viewer,
            ..DiplomacyView::default()
        },
    )))
}
