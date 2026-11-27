use crate::{EntityId, GroundRelic, Order, RelicAction, RelicView, WorldPosition};
use aoe_protocol::{GameplayClientMessage as C, GameplayServerMessage as S};

#[test]
fn exposes_relics_only_for_the_active_subscription_and_sends_live_orders() {
    let mut world = super::world::started();
    let relic = GroundRelic {
        id: EntityId(4),
        position: WorldPosition::new(3, 5),
    };
    let view = RelicView {
        revision: 1,
        ground: vec![relic],
        ..RelicView::default()
    };
    world.apply(S::Relics(view.clone()));
    assert_eq!(world.relics(), Some(&view));
    world.subscription += 1;
    world.apply(S::Relics(view));
    assert!(world.relics().is_none());
    let action = RelicAction::PickUp {
        monk: EntityId(1),
        relic: relic.id,
    };
    assert_eq!(
        Order::Relic(action).message(7),
        Some(C::RelicOrder {
            sequence: 7,
            action
        })
    );
}
