//! This stationary fixture keeps its accepted revision-1 subscription after errors.
//! Live ticks may precede a response; no other unsolicited message is permitted.
use super::{GameplayServerMessage, Socket, receive};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

// The server's outgoing session queue holds 64 messages. This is an additional
// finite work bound, not permission to extend the original two-second deadline.
const FRAME_BUDGET: usize = 64;

fn classify(message: &GameplayServerMessage, expected_code: u16) -> Result<bool, String> {
    match message {
        GameplayServerMessage::Error { code, .. } if *code == expected_code => Ok(true),
        GameplayServerMessage::Tick {
            revision: 1,
            changed_units,
            removals,
            ..
        } if changed_units.is_empty() && removals.is_empty() => Ok(false),
        _ => Err(format!(
            "expected subscription Error {expected_code} or an empty revision-1 Tick, received {message:?}"
        )),
    }
}

pub(super) async fn receive_error(socket: &mut Socket, expected_code: u16) {
    let deadline = Instant::now() + Duration::from_secs(2);
    for _ in 0..FRAME_BUDGET {
        let message = timeout_at(deadline, receive(socket))
            .await
            .unwrap_or_else(|_| {
                panic!("subscription Error {expected_code} missing at total deadline")
            });
        match classify(&message, expected_code) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => panic!("{error}"),
        }
    }
    panic!("subscription Error {expected_code} missing after {FRAME_BUDGET} frames");
}

#[test]
fn subscription_response_classification_accepts_only_expected_error_or_stationary_tick() {
    use aoe_core::Tick;
    let tick = GameplayServerMessage::Tick {
        revision: 1,
        tick: Tick(1),
        changed_units: vec![],
        removals: vec![],
    };
    for code in [409, 400] {
        assert_eq!(classify(&tick, code), Ok(false));
        let error = GameplayServerMessage::Error {
            code,
            message: "subscription rejected".into(),
        };
        assert_eq!(classify(&error, code), Ok(true));
        assert!(classify(&error, if code == 409 { 400 } else { 409 }).is_err());
    }
}

#[test]
fn subscription_response_classification_rejects_changed_revision_or_nonstationary_ticks() {
    use aoe_core::{EntityId, PlayerId, Tick, WorldPosition};
    use aoe_protocol::GameplayUnitState;
    let unit = GameplayUnitState {
        id: EntityId(0),
        player: PlayerId(0),
        position: WorldPosition::new(0, 0),
        moving: false,
        planning: false,
        facing: 0,
    };
    let messages = [
        GameplayServerMessage::Tick {
            revision: 2,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![],
        },
        GameplayServerMessage::Tick {
            revision: 0,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![],
        },
        GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(1),
            changed_units: vec![unit],
            removals: vec![],
        },
        GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![EntityId(0)],
        },
    ];
    for message in messages {
        let error = classify(&message, 400).expect_err("unexpected tick must not be drained");
        assert!(error.contains("expected subscription Error 400"));
        assert!(error.contains("received Tick"));
    }
}

#[test]
fn subscription_response_classification_does_not_hide_other_server_messages() {
    use aoe_core::Tick;
    use aoe_protocol::CommandResult;
    let messages = [
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(1),
            units: vec![],
        },
        GameplayServerMessage::CommandAck {
            sequence: 1,
            result: CommandResult::Accepted,
            applied_tick: Tick(1),
        },
        GameplayServerMessage::WorldReset { world_id: 2 },
        GameplayServerMessage::Error {
            code: 429,
            message: "rate limited".into(),
        },
    ];
    for message in messages {
        let error = classify(&message, 409).expect_err("unrelated server message must fail");
        assert!(error.contains("expected subscription Error 409"));
        assert!(error.contains(&format!("received {message:?}")));
    }
}
