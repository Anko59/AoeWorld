use super::*;
use aoe_core::{Seed, TileCoord};
use aoe_simulation::TileVisibility;

#[tokio::test]
async fn viewer_receives_a_name_and_only_their_own_allies_market_positions() {
    let player = PlayerId(0);
    let ally = PlayerId(1);
    let service = super::super::GameplayService::diplomacy_demo(Seed(7), true).unwrap();
    let mut world = service.world.write().await;
    world
        .set_diplomatic_stance(player, ally, DiplomaticStance::Ally)
        .unwrap();
    let view = view(&world, &service.slots, player, 4, false).unwrap();
    assert_eq!(view.seats[1].name.as_deref(), Some("Lyon"));
    assert_eq!(view.stance(ally), DiplomaticStance::Ally);
    let tile = TileCoord::new(192, 192);
    assert_eq!(view.markets, [AlliedMarket { player: ally, tile }]);
    assert!(view.tribute_market.is_some());
    assert_eq!(world.visibility(player, tile), TileVisibility::Unexplored);
    world
        .set_diplomatic_stance(player, ally, DiplomaticStance::Neutral)
        .unwrap();
    assert!(
        super::view(&world, &service.slots, player, 4, false)
            .unwrap()
            .markets
            .is_empty()
    );
}

#[tokio::test]
async fn ai_resubscription_replaces_a_withdrawn_offer_with_an_empty_view() {
    let service = super::super::GameplayService::diplomacy_demo(Seed(7), false).unwrap();
    let human = PlayerId(0);
    let ai = PlayerId(1);
    service
        .world
        .write()
        .await
        .offer_alliance(human, ai)
        .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    let (session, _) = service
        .join_slot(service.slots[1].token, sender)
        .await
        .unwrap();
    let region = aoe_core::TileRect::from_xywh(0, 0, 256, 256);
    service.subscribe(session, 1, region, true).await;
    let mut diplomacy = None;
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::Diplomacy(view) = message {
            diplomacy = Some(view);
        }
    }
    assert_eq!(diplomacy.unwrap().seats[0].offer, AllianceOffer::Received);

    service
        .world
        .write()
        .await
        .set_diplomatic_stance(human, ai, DiplomaticStance::Neutral)
        .unwrap();
    service.subscribe(session, 2, region, true).await;
    while let Ok(message) = receiver.try_recv() {
        if let GameplayServerMessage::Diplomacy(view) = message {
            assert_eq!(
                view,
                DiplomacyView {
                    revision: 2,
                    viewer: ai,
                    ..Default::default()
                }
            );
            return;
        }
    }
    panic!("AI retained the withdrawn alliance offer");
}
