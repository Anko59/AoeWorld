use super::*;
use aoe_core::Seed;
use tokio::sync::mpsc;

#[tokio::test]
async fn pages_share_a_seat_and_retire_and_delivery_are_observable() {
    let service = GameplayService::new(Seed(1));
    assert!(service.world_id() > 0);

    let (first_tx, mut first_rx) = mpsc::channel(8);
    let (first_id, welcome) = service.register(None, first_tx).await;
    let token = match welcome {
        GameplayServerMessage::Welcome { resume_token, .. } => resume_token,
        message => panic!("unexpected welcome: {message:?}"),
    };
    assert!(service.is_human_token(token));

    let (second_tx, mut second_rx) = mpsc::channel(8);
    let (second_id, welcome) = service.register(None, second_tx).await;
    assert!(matches!(
        welcome,
        GameplayServerMessage::Welcome { resume_token, .. } if resume_token == token
    ));

    service
        .send_to(
            second_id,
            GameplayServerMessage::Error {
                code: 400,
                message: "test delivery".to_owned(),
            },
        )
        .await;
    assert!(matches!(
        second_rx.try_recv().expect("second page message"),
        GameplayServerMessage::Error { code: 400, .. }
    ));
    assert!(first_rx.try_recv().is_err());

    service.disconnect(first_id).await;
    assert_eq!(service.presence().await, (0, 0, true));

    service.retire(99).await;
    assert!(matches!(
        second_rx.try_recv().expect("retirement reset"),
        GameplayServerMessage::WorldReset { world_id: 99 }
    ));
    service
        .send_to(
            second_id,
            GameplayServerMessage::Error {
                code: 500,
                message: "after retire".to_owned(),
            },
        )
        .await;
    assert!(second_rx.try_recv().is_err());
}
