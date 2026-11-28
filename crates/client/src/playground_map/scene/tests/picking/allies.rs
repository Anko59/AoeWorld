use super::*;
use crate::playground::game::village::{diplomacy, owner};
use aoe_protocol::{DiplomacySeat, DiplomacyView, DiplomaticStance};

pub(super) fn verify_attackability(client: &mut Client, enemy: PlayerId, point: ScreenPoint) {
    let viewer = owner(client);
    for (stance, attackable) in [
        (DiplomaticStance::Neutral, true),
        (DiplomaticStance::Ally, false),
    ] {
        diplomacy::sync(
            client,
            DiplomacyView {
                viewer,
                seats: vec![DiplomacySeat {
                    player: enemy,
                    name: None,
                    stance,
                    offer: aoe_protocol::AllianceOffer::None,
                }],
                ..Default::default()
            },
        );
        assert_eq!(
            picking::actor(client, point),
            Some((EntityId(1), false, attackable))
        );
    }
}
