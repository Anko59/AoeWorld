use super::*;
use aoe_core::Seed;

#[tokio::test]
async fn an_unauthenticated_page_cannot_take_an_ai_seat_when_no_humans_exist() {
    let (world, primary) = GameWorld::default_with_village(Seed(7));
    let service = GameplayService::from_world_slots(
        world,
        vec![SlotSetup {
            player: PlayerId(0),
            primary_unit_id: primary,
            human: false,
        }],
        None,
        None,
    );
    for access in [
        Access::Hello(None),
        Access::Hello(Some(ResumeToken([255; 24]))),
    ] {
        let (sender, _) = mpsc::channel(8);
        assert!(service.register_access(access, sender).await.is_err());
        assert!(service.sessions.lock().await.is_empty());
    }
    let token = service.slot_infos().await[0].token;
    let (sender, _) = mpsc::channel(8);
    assert!(
        service
            .register_access(Access::Join(token), sender)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn order_decisions_are_attributed_to_the_authenticated_seat_kind() {
    use aoe_simulation::OrderIssuer;
    for human in [true, false] {
        let (world, primary) = GameWorld::default_with_village(Seed(7));
        let service = GameplayService::from_world_slots(
            world,
            vec![SlotSetup {
                player: PlayerId(0),
                primary_unit_id: primary,
                human,
            }],
            None,
            None,
        );
        let token = service.slot_infos().await[0].token;
        let (sender, _receiver) = mpsc::channel(32);
        let (session, _) = service
            .register_access(Access::Join(token), sender)
            .await
            .unwrap();
        // Stop is an order even though it never requests a new route.
        service.queue_stop(session, 1, primary).await;
        service.tick().await;
        let world = service.world.read().await;
        let issuer = if human {
            OrderIssuer::Player
        } else {
            OrderIssuer::PythonAi
        };
        assert_eq!(world.order_demand()[issuer as usize].decisions, 1);
        assert_eq!(world.order_demand()[issuer as usize].route_requests, 0);
        let other = if human {
            OrderIssuer::PythonAi
        } else {
            OrderIssuer::Player
        };
        assert_eq!(world.order_demand()[other as usize].decisions, 0);
    }
}
