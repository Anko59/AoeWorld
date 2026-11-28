use crate::tests::{
    fake::{FakeWorld, base},
    play::play_fresh,
};
use aoe_sdk::{AllianceOffer, DiplomacySeat, DiplomacyView, DiplomaticStance, Order, PlayerId};

#[test]
fn brain_accepts_an_incoming_alliance_through_the_ordinary_order_path() {
    let (units, buildings) = base(6);
    let mut world = FakeWorld::new(units, buildings);
    world.diplomacy = Some(DiplomacyView {
        revision: 1,
        seats: vec![
            DiplomacySeat {
                player: PlayerId(1),
                name: None,
                stance: DiplomaticStance::Enemy,
                offer: AllianceOffer::Received,
            },
            DiplomacySeat {
                player: PlayerId(2),
                name: None,
                stance: DiplomaticStance::Enemy,
                offer: AllianceOffer::Sent,
            },
        ],
        ..DiplomacyView::default()
    });
    let (_, recorder) = play_fresh(&world);
    let offers: Vec<_> = recorder
        .orders
        .iter()
        .filter_map(|order| match order {
            Order::Diplomacy {
                player,
                stance: DiplomaticStance::Ally,
            } => Some(*player),
            _ => None,
        })
        .collect();
    assert_eq!(offers, [PlayerId(1)]);
}
