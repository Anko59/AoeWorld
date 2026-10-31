use super::*;
use aoe_core::Seed;

#[path = "tests/combat.rs"]
mod combat;

#[tokio::test]
async fn outgoing_validation_and_socket_failures_are_distinguishable() {
    let mut sink = futures_util::sink::drain();
    let invalid = GameplayServerMessage::Projectiles {
        revision: 0,
        tick: aoe_core::Tick(0),
        projectiles: vec![],
    };
    assert!(matches!(
        send_socket(&mut sink, invalid).await,
        Err(SendFailure::Encode(_))
    ));
    let mut broken = Box::pin(futures_util::sink::unfold((), |(), _| async {
        Err::<(), ()>(())
    }));
    assert!(matches!(
        send_socket(
            &mut broken,
            GameplayServerMessage::Error {
                code: 400,
                message: "invalid order".into()
            }
        )
        .await,
        Err(SendFailure::Write)
    ));
}

#[test]
fn tokens_are_unpredictable_even_for_the_same_world_and_counter() {
    assert_ne!(make_token(42, 7), make_token(42, 7));
    assert_ne!(make_token(42, 7), make_token(42, 8));
}

#[tokio::test]
async fn a_token_of_another_world_seats_a_page_under_this_worlds_own_token() {
    let retired = GameplayService::new(Seed(1));
    let stale = retired.slot_infos().await[0].token;
    let current = GameplayService::new(Seed(2));
    let (tx, _rx) = mpsc::channel(8);
    let (_, welcome) = current.register(Some(stale), tx).await;
    let GameplayServerMessage::Welcome { resume_token, .. } = welcome else {
        panic!("a page always receives a welcome");
    };
    assert_ne!(resume_token, stale, "the stale credential is never adopted");
    assert!(current.is_human_token(resume_token));
    assert!(!current.is_human_token(stale));
}
