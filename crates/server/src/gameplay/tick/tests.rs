//! The fan-out sends every session exactly the bytes it always did.
use super::scenario::{SEATS, SessionLog, run};
use aoe_core::PlayerId;
use aoe_protocol::{FogState, GameplayServerMessage, encode_gameplay_server};
use aoe_simulation::{GameWorld, TileVisibility};

/// FNV-1a over every encoded message, in order; `(messages, digest)` per session.
/// A fog view with the same content the client already holds is not a
/// different experience for it, so a repeat of one is left out.
fn digests(logs: &[SessionLog]) -> Vec<(usize, u64)> {
    logs.iter()
        .map(|log| {
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            let mut held: Option<&FogState> = None;
            let mut count = 0;
            for message in &log.messages {
                if let GameplayServerMessage::FogState(fog) = message {
                    let repeat = held.is_some_and(|held| {
                        held.revision == fog.revision
                            && held.region == fog.region
                            && held.disabled == fog.disabled
                            && held.tiles == fog.tiles
                            && held.overview == fog.overview
                    });
                    held = Some(fog);
                    if repeat {
                        continue;
                    }
                }
                count += 1;
                for byte in encode_gameplay_server(message).unwrap() {
                    hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                }
                hash = (hash ^ 0xff).wrapping_mul(0x0100_0000_01b3);
            }
            (count, hash)
        })
        .collect()
}

/// Computed with the per-session fan-out before it was rebuilt; regenerate from
/// the printed line only when the wire output changes on purpose.
const PINNED: [(usize, u64); SEATS + 1] = [
    (407, 4780852846789625595),
    (413, 5820668468973553449),
    (51, 9022699601399011410),
    (168, 10913971377557851011),
    (170, 9229010762936698779),
    (407, 1662405826965071658),
];

#[tokio::test(flavor = "multi_thread")]
async fn per_session_messages_match_the_pinned_stream() {
    let logs = run(400, &mut |_, _, _| {}).await;
    let actual = digests(&logs);
    println!("PINNED: {actual:?}");
    println!(
        "messages per session: {:?}",
        logs.iter().map(|l| l.messages.len()).collect::<Vec<_>>()
    );
    let kinds = |f: fn(&GameplayServerMessage) -> bool| {
        logs.iter()
            .flat_map(|l| &l.messages)
            .filter(|m| f(m))
            .count()
    };
    println!(
        "ticks {} fog {} wildlife {} projectiles {}",
        kinds(|m| matches!(m, GameplayServerMessage::Tick { .. })),
        kinds(|m| matches!(m, GameplayServerMessage::FogState(_))),
        kinds(|m| matches!(m, GameplayServerMessage::WildlifeState { .. })),
        kinds(|m| matches!(m, GameplayServerMessage::Projectiles { .. })),
    );
    let foreign_units: usize = logs
        .iter()
        .flat_map(|log| log.messages.iter().map(move |m| (log.viewer, m)))
        .map(|(viewer, message)| match message {
            GameplayServerMessage::Tick { changed_units, .. } => {
                changed_units.iter().filter(|u| u.player != viewer).count()
            }
            _ => 0,
        })
        .sum();
    println!("foreign units sent {foreign_units}");
    assert!(foreign_units > 0, "the scenario never crosses views");
    assert_eq!(actual, PINNED);
}

/// Whatever a session is sent about other players stands in sight of its
/// viewer at the moment it is sent, or is a building its viewer remembers.
fn assert_nothing_unseen(world: &GameWorld, viewer: PlayerId, messages: &[GameplayServerMessage]) {
    let visible = |at: aoe_core::WorldPosition| {
        world.visibility(viewer, at.tile_floor()) == TileVisibility::Visible
    };
    for message in messages {
        match message {
            GameplayServerMessage::Tick {
                changed_units,
                changed_players,
                changed_buildings,
                ..
            } => {
                for unit in changed_units {
                    assert!(
                        unit.player == viewer || visible(unit.position),
                        "{viewer:?} was sent {:?} of {:?} out of sight",
                        unit.id,
                        unit.player
                    );
                }
                assert!(changed_players.iter().all(|p| p.player == viewer));
                let known = world.known_buildings(viewer);
                for building in changed_buildings {
                    assert!(
                        building.player == viewer || known.iter().any(|b| b.id == building.id),
                        "{viewer:?} was sent building {:?} it never saw",
                        building.id
                    );
                }
            }
            GameplayServerMessage::WildlifeState { animals, .. } => {
                for animal in animals {
                    assert!(animal.owner == Some(viewer) || visible(animal.position));
                }
            }
            GameplayServerMessage::Projectiles { projectiles, .. } => {
                for projectile in projectiles {
                    assert!(visible(projectile.from), "a projectile appeared in the fog");
                    assert!(projectile.to == projectile.from || visible(projectile.to));
                }
            }
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_is_never_sent_what_its_player_cannot_see() {
    let mut checked = 0;
    run(400, &mut |world, viewer, messages| {
        checked += messages.len();
        assert_nothing_unseen(world, viewer, messages);
    })
    .await;
    assert!(checked > 1_000, "the scenario sent {checked} messages");
}
