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
