use super::world::{started, unit};
use crate::{
    AllianceOffer, DiplomacySeat, DiplomacyView, DiplomaticStance, EntityId, Order, PlayerId,
    UnitKind,
};
use aoe_protocol::{GameplayClientMessage as C, GameplayServerMessage as S};

#[test]
fn diplomacy_filters_combat_queries_without_hiding_the_allied_unit() {
    let mut world = started();
    let other = PlayerId(1);
    world
        .units
        .insert(EntityId(10), unit(10, other, UnitKind::Militia));
    assert_eq!(world.visible_enemies().len(), 1);
    let view = DiplomacyView {
        revision: world.subscription,
        viewer: world.me().unwrap(),
        seats: vec![DiplomacySeat {
            player: other,
            name: None,
            stance: DiplomaticStance::Ally,
            offer: AllianceOffer::None,
        }],
        ..DiplomacyView::default()
    };
    world.apply(S::Diplomacy(view));
    assert!(world.visible_enemies().is_empty());
    assert!(world.unit(EntityId(10)).is_some());
    assert_eq!(
        Order::Diplomacy {
            player: other,
            stance: DiplomaticStance::Ally
        }
        .message(7),
        Some(C::DiplomacyOrder {
            sequence: 7,
            player: other,
            stance: DiplomaticStance::Ally
        }),
    );
    world.subscription += 1;
    assert!(world.visible_enemies().is_empty());
    world.apply(S::Diplomacy(DiplomacyView {
        revision: world.subscription - 1,
        viewer: world.me().unwrap(),
        ..DiplomacyView::default()
    }));
    assert!(world.visible_enemies().is_empty());
}
