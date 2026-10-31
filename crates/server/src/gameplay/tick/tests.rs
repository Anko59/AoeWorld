// Radius geometry, exact static sweeps and local rejoining intentionally
// change these current pins. Preserve workloads, floors and replay comparisons.
//! Pin the current deterministic state stream delivered to every session.
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

/// Current per-session wire digests include prepaid Mill reseeds, auto-scout,
/// unit cargo_count and building garrisons; routes pass through open TC arches.
/// The playtest bundle deposits carried loads on contact with any building side,
/// cancelling return movement earlier. Swept avoidance, nearest-free arrivals,
/// bounded retries and unchanged-wait suppression further change movement/fog
/// timing. Regenerate all six combined streams for the same 400-tick scenario.
/// In this combination sessions 1 and 4 change from 171/410 to 245/422 messages;
/// the other four counts and digests remain unchanged from the playtest bundle.
/// Regenerated from `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server
/// AOE_NATIVE_TEST_FILTER='test(per_session_messages_match_the_pinned_stream)'`;
/// keep the scenario, all six sessions and every visibility assertion unchanged.
/// Combined bundle: gatherer reservations change routes, loads and message
/// counts; water appends bridge spans and Vassal appends home/control IDs.
/// Regenerated from the focused current encoder stream. The scenario, digest
/// algorithm and independent replay/visibility assertions remain unchanged.
/// Combat acquisition, stances and retaliation change the observed encounters;
/// regenerated with the same scenario.
/// Rebased onto #573: avoidance, arrival slots and contact drop-off combine
/// with gatherer reservations; sessions 1, 4 and 5 change (409/407/407 to
/// 138/421/407 messages). Regenerated with the focused command above.
/// Removing the unused RoleChange message shifts every later server message tag
/// by one, so all six digests changed; counts and assertions are unchanged.
/// General commander/army fields extend the current unit wire. Message counts
/// and every workload, replay and visibility assertion stay unchanged.
const PINNED: [(usize, u64); SEATS + 1] = [
    (407, 6205794337568153004),
    // PR 10 legal work contact changes gathering and fog updates.
    (187, 16495370670290857238),
    (60, 1780256642041405484),
    (176, 13848062883790336040),
    // PR 10 legal contacts and unavailable land-work queues change this stream.
    (420, 16572561531079986710),
    (407, 2183301173389447027),
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
                    // A shooter is named, and the true launch kept, only while
                    // its firing position is in sight.
                    assert!(projectile.attacker.is_none() || visible(projectile.from));
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
