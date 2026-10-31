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
