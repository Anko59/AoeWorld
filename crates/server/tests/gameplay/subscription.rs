//! This stationary fixture keeps its accepted revision-1 subscription after errors.
//! Idle ticks, the black fog view, changes that leave it black and owned sheep
//! can precede an error response.
use super::{GameplayServerMessage, Socket, receive};
use aoe_protocol::{FogDelta, FogState};
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

// The server's outgoing session queue holds 64 messages. This is an additional
// finite work bound, not permission to extend the original two-second deadline.
const FRAME_BUDGET: usize = 64;

/// The fog view this fixture holds: revision 1, the one tile at the origin, black.
fn black_view(state: &FogState) -> bool {
    state.revision == 1
        && state.region == aoe_core::TileRect::from_xywh(0, 0, 1, 1)
        && state.tiles == [0]
}

/// The view the fixture holds once `delta` is laid over it is still that black
/// view: the strategic overview may change (an owned sheep wanders), the tile
/// may not, and changes for another revision or window are refused.
fn stays_black(delta: &FogDelta) -> bool {
    let mut held = FogState {
        revision: 1,
        tick: aoe_core::Tick(0),
        region: aoe_core::TileRect::from_xywh(0, 0, 1, 1),
        disabled: false,
        tiles: vec![0],
        overview: Vec::new(),
    };
    held.apply(delta).is_ok() && black_view(&held)
}

fn classify(message: &GameplayServerMessage, expected_code: u16) -> Result<bool, String> {
    match message {
        GameplayServerMessage::Error { code, .. } if *code == expected_code => Ok(true),
        GameplayServerMessage::FogState(state) if black_view(state) => Ok(false),
        GameplayServerMessage::FogDelta(delta) if stays_black(delta) => Ok(false),
        GameplayServerMessage::WildlifeState {
            revision: 1,
            animals,
            ..
        } if animals
            .iter()
            .all(|a| a.owner == Some(aoe_core::PlayerId(0))) =>
        {
            Ok(false)
        }
        GameplayServerMessage::Projectiles {
            revision: 1,
            projectiles,
            ..
        } if projectiles.is_empty() => Ok(false),
        GameplayServerMessage::Holdings { revision: 1, .. } => Ok(false),
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
        changed_players: Vec::new(),
        changed_buildings: Vec::new(),
        building_removals: Vec::new(),
    };
    for code in [409, 400] {
        assert_eq!(classify(&tick, code), Ok(false));
        assert_eq!(
            classify(
                &GameplayServerMessage::Holdings {
                    revision: 1,
                    tick: Tick(1),
                    shapes: vec![],
                    complete: true
                },
                code
            ),
            Ok(false)
        );
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
        kind: aoe_protocol::UnitKind::Scout,
        task: aoe_protocol::UnitTask::Idle,
        carried: None,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        sergeant: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };
    let messages = [
        GameplayServerMessage::Holdings {
            revision: 2,
            tick: Tick(1),
            shapes: vec![],
            complete: true,
        },
        GameplayServerMessage::Tick {
            revision: 2,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![],
            changed_players: Vec::new(),
            changed_buildings: Vec::new(),
            building_removals: Vec::new(),
        },
        GameplayServerMessage::Tick {
            revision: 0,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![],
            changed_players: Vec::new(),
            changed_buildings: Vec::new(),
            building_removals: Vec::new(),
        },
        GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(1),
            changed_units: vec![unit],
            removals: vec![],
            changed_players: Vec::new(),
            changed_buildings: Vec::new(),
            building_removals: Vec::new(),
        },
        GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(1),
            changed_units: vec![],
            removals: vec![EntityId(0)],
            changed_players: Vec::new(),
            changed_buildings: Vec::new(),
            building_removals: Vec::new(),
        },
    ];
    for message in messages {
        let error = classify(&message, 400).expect_err("unexpected tick must not be drained");
        assert!(error.contains("expected subscription Error 400"));
        assert!(error.contains(&format!("received {message:?}")));
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
            players: Vec::new(),
            buildings: Vec::new(),
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

#[test]
fn subscription_response_classification_accepts_fog_only_while_the_view_stays_black() {
    use aoe_core::{Tick, TileRect};
    let view = |revision: u64, tiles: &[u8], overview: &[(u16, u8)]| FogState {
        revision,
        tick: Tick(1),
        region: TileRect::from_xywh(0, 0, tiles.len() as i32, 1),
        disabled: false,
        tiles: tiles.to_vec(),
        overview: overview.to_vec(),
    };
    let changes = |held: &FogState, fresh: &FogState| {
        GameplayServerMessage::FogDelta(held.delta_to(fresh).expect("the same view"))
    };
    // An owned sheep wanders: strategic pixels appear, change and go black.
    let black = view(1, &[0], &[]);
    let sheep = view(1, &[0], &[(7, 2)]);
    let wandered = view(1, &[0], &[(8, 2)]);
    let accepted = [
        GameplayServerMessage::FogState(black.clone()),
        GameplayServerMessage::FogState(sheep.clone()),
        changes(&black, &sheep),
        changes(&sheep, &wandered),
    ];
    for message in &accepted {
        for code in [409, 400] {
            assert_eq!(classify(message, code), Ok(false), "{message:?}");
        }
    }
    let lit = view(1, &[2], &[]);
    let other_revision = view(2, &[0], &[]);
    let rejected = [
        GameplayServerMessage::FogState(lit.clone()),
        GameplayServerMessage::FogState(other_revision.clone()),
        GameplayServerMessage::FogState(view(1, &[0, 0], &[])),
        // The tile is no longer black, with or without a strategic change.
        changes(&black, &lit),
        changes(&sheep, &view(1, &[1], &[(8, 2)])),
        // Changes of another subscription revision.
        changes(&other_revision, &view(2, &[0], &[(7, 2)])),
        // Changes to a tile beside the fixture's, or further along its row.
        changes(&view(1, &[0, 0], &[]), &view(1, &[0, 2], &[])),
        changes(&view(1, &[0; 5], &[]), &view(1, &[0, 0, 0, 0, 1], &[])),
    ];
    for message in rejected {
        let error = classify(&message, 400).expect_err("fog of another view must not be drained");
        assert!(error.contains("expected subscription Error 400"));
        assert!(error.contains(&format!("received {message:?}")));
    }
}
