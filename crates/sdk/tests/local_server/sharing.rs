use super::*;
use aoe_sdk::{CommandResult, Event};
use std::time::Duration;

async fn accepted(client: &Client, sequence: u64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let decision = client.next_decision().await.unwrap();
            for event in decision.events {
                if let Event::OrderResult {
                    sequence: seen,
                    result,
                } = event
                    && seen == sequence
                {
                    assert_eq!(result, CommandResult::Accepted);
                    return;
                }
            }
        }
    })
    .await
    .expect("the connected client receives its own command result");
}

#[tokio::test]
async fn clients_share_one_seat_without_replacing_each_other() {
    let address = server().await;
    let url = format!("http://{address}");
    let info = start_match(&url, SECRET, &MatchMap::flat(500, 3), &[SlotKind::Ai])
        .await
        .unwrap();
    let token = &info.slots[0].token;
    let first = Client::connect(&url, token, ConnectOptions::default())
        .await
        .unwrap();
    let first_view = first.next_decision().await.unwrap();
    let unit = first_view.world.my_units(None)[0].id();
    let second = Client::connect(&url, token, ConnectOptions::default())
        .await
        .unwrap();
    let second_view = second.next_decision().await.unwrap();
    assert!(
        second_view
            .world
            .my_units(None)
            .iter()
            .any(|u| u.id() == unit)
    );

    let first_sequence = first.send(Order::Stop { unit });
    let second_sequence = second.send(Order::Stop { unit });
    assert_eq!(
        first_sequence, second_sequence,
        "sequences belong to sockets"
    );
    tokio::join!(
        accepted(&first, first_sequence),
        accepted(&second, second_sequence)
    );
    assert_eq!(first.connection_stats().drops, 0);
    assert_eq!(second.connection_stats().drops, 0);

    first.close().await;
    let sequence = second.send(Order::Stop { unit });
    accepted(&second, sequence).await;
    assert_eq!(second.connection_stats().drops, 0);
    second.close().await;
}
