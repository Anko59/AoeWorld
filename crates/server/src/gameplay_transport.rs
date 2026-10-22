use crate::gameplay::{Access, ControllerLease, GameplayService, Ownership, Session};
use aoe_protocol::{
    GAMEPLAY_VERSION, GameplayClientMessage, GameplayRole, GameplayServerMessage, ResumeToken,
    decode_gameplay_client, encode_gameplay_server,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::mpsc, time::timeout};

const LEASE_RESERVATION: Duration = Duration::from_secs(30);
/// How long one socket write may take before the peer counts as gone: long
/// enough for a bot busy deciding, short enough to free a dead connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) async fn handle_socket(service: GameplayService, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let hello = timeout(Duration::from_secs(5), stream.next()).await;
    let Some(Ok(Message::Binary(bytes))) = hello.ok().flatten() else {
        return;
    };
    let access = match decode_gameplay_client(&bytes) {
        Ok(GameplayClientMessage::Hello {
            version: GAMEPLAY_VERSION,
            resume_token,
        }) => Access::Hello(resume_token),
        Ok(GameplayClientMessage::JoinSlot {
            version: GAMEPLAY_VERSION,
            token,
        }) => Access::Join(token),
        _ => {
            let _ = send_socket(
                &mut sink,
                GameplayServerMessage::Error {
                    code: 426,
                    message: "gameplay protocol version or hello mismatch".to_owned(),
                },
            )
            .await;
            return;
        }
    };
    let (sender, mut receiver) = mpsc::channel(64);
    let Ok((session_id, welcome)) = service.register_access(access, sender.clone()).await else {
        let _ = send_socket(
            &mut sink,
            GameplayServerMessage::Error {
                code: 403,
                message: "unknown slot token".to_owned(),
            },
        )
        .await;
        return;
    };
    if sender.send(welcome).await.is_err() {
        return;
    }
    drop(sender);
    loop {
        tokio::select! {
            outgoing = receiver.recv() => {
                let Some(message) = outgoing else { break };
                let reset = matches!(message, GameplayServerMessage::WorldReset { .. });
                if send_socket(&mut sink, message).await.is_err() {
                    tracing::warn!(session_id, "gameplay session dropped: a socket write failed or timed out");
                    break;
                }
                if reset { break }
            }
            incoming = stream.next() => {
                let Some(Ok(Message::Binary(bytes))) = incoming else { break };
                match decode_gameplay_client(&bytes) {
                    Ok(GameplayClientMessage::Subscribe { revision, region }) => service.subscribe(session_id, revision, region).await,
                    Ok(GameplayClientMessage::MoveOrder { sequence, entity_id, destination }) => service.queue_move(session_id, sequence, entity_id, destination).await,
                    Ok(GameplayClientMessage::StopOrder { sequence, entity_id }) => service.queue_stop(session_id, sequence, entity_id).await,
                    Ok(GameplayClientMessage::GatherOrder { sequence, entity_id, resource_id }) => service.queue_gather(session_id, sequence, entity_id, resource_id).await,
                    Ok(GameplayClientMessage::CancelTrainingOrder { sequence, building_id }) => service.queue_cancel_training(session_id, sequence, building_id).await,
                    Ok(GameplayClientMessage::TrainOrder { sequence, building_id, kind }) => service.queue_train(session_id, sequence, building_id, kind).await,
                    Ok(GameplayClientMessage::AttackOrder { sequence, entity_id, target_id }) => service.queue_attack(session_id, sequence, entity_id, target_id).await,
                    Ok(GameplayClientMessage::BuildOrder { sequence, builder_ids, kind, origin }) => service.queue_build(session_id, sequence, builder_ids, kind, origin).await,
                    Ok(GameplayClientMessage::BuildWallOrder { sequence, builder_ids, from, to }) => service.queue_build_wall(session_id, sequence, builder_ids, from, to).await,
                    Ok(GameplayClientMessage::CancelConstructionOrder { sequence, building_id }) => service.queue_cancel_construction(session_id, sequence, building_id).await,
                    Ok(GameplayClientMessage::ConstructOrder { sequence, builder_ids, building_id }) => service.queue_construct(session_id, sequence, builder_ids, building_id).await,
                    Ok(GameplayClientMessage::FarmOrder { sequence, entity_id, farm_id }) => service.queue_farm(session_id, sequence, entity_id, farm_id).await,
                    Ok(GameplayClientMessage::ResearchOrder { sequence, building_id, tech }) => service.queue_research(session_id, sequence, building_id, tech).await,
                    Ok(GameplayClientMessage::CancelResearch { sequence, building_id }) => service.queue_cancel_research(session_id, sequence, building_id).await,
                    Ok(GameplayClientMessage::HuntOrder { sequence, unit_ids, animal_id }) => service.queue_hunt(session_id, sequence, unit_ids, animal_id).await,
                    Ok(GameplayClientMessage::MarketBuy { sequence, building_id, resource }) => service.queue_market_buy(session_id, sequence, building_id, resource).await,
                    Ok(GameplayClientMessage::MarketSell { sequence, building_id, resource }) => service.queue_market_sell(session_id, sequence, building_id, resource).await,
                    Ok(GameplayClientMessage::Resync { revision }) => service.resync(session_id, revision).await,
                    Ok(GameplayClientMessage::Hello { .. } | GameplayClientMessage::JoinSlot { .. }) | Err(_) => {
                        let _ = send_socket(&mut sink, GameplayServerMessage::Error { code: 400, message: "invalid gameplay request".to_owned() }).await;
                        break;
                    }
                }
            }
        }
    }
    service.disconnect(session_id).await;
}

async fn send_socket<S>(sink: &mut S, message: GameplayServerMessage) -> Result<(), ()>
where
    S: Sink<Message> + Unpin,
{
    let bytes = encode_gameplay_server(&message).map_err(|_| ())?;
    timeout(SEND_TIMEOUT, sink.send(Message::Binary(bytes.into())))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

/// Gives every human slot whose controller stayed away past the lease to the
/// oldest spectator, under a fresh token; an empty slot stays claimable under a fresh token.
/// AI slots are never reassigned: only their token reaches them.
pub(super) fn promote_expired(
    ownership: &mut Ownership,
    sessions: &mut BTreeMap<u64, Session>,
    world_id: u64,
) {
    for index in 0..ownership.slots.len() {
        let slot = &ownership.slots[index];
        let expired = slot.human
            && slot.lease.is_some_and(|lease| {
                lease
                    .disconnected_at
                    .is_some_and(|at| at.elapsed() >= LEASE_RESERVATION)
            });
        if !expired {
            continue;
        }
        let next = sessions
            .iter()
            .filter(|(_, session)| session.role == GameplayRole::Spectator)
            .min_by_key(|(_, session)| session.connected_at)
            .map(|(id, _)| *id);
        let token = make_token(world_id, ownership.next_token);
        ownership.next_token = ownership.next_token.wrapping_add(1);
        let Some(session_id) = next else {
            // Nobody to promote: the seat opens, and the departed player's
            // token must not open it again or hand it to the next claimant.
            ownership.slots[index].lease = None;
            ownership.slots[index].token = token;
            continue;
        };
        let slot = &mut ownership.slots[index];
        slot.token = token;
        slot.lease = Some(ControllerLease {
            session_id,
            disconnected_at: None,
        });
        if let Some(session) = sessions.get_mut(&session_id) {
            session.role = GameplayRole::Controller;
            session.token = Some(token);
            session.slot = Some(index);
            session.ai = false;
            if session.viewer != slot.player {
                session.change_viewer(slot.player);
            }
            session.primary_unit_id = slot.primary_unit_id;
            let _ = session.sender.try_send(GameplayServerMessage::RoleChange {
                role: GameplayRole::Controller,
                primary_unit_id: slot.primary_unit_id,
                resume_token: Some(token),
            });
        }
    }
}

/// An unpredictable 192-bit token. `world_id` and `counter` only diversify the
/// hasher input; the secrecy comes from the per-hasher random keys.
pub(super) fn make_token(world_id: u64, counter: u64) -> ResumeToken {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0_u8; 24];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(world_id);
        hasher.write_u64(counter);
        hasher.write_u64(index as u64);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    ResumeToken(bytes)
}

#[path = "gameplay_transport/tests.rs"]
#[cfg(test)]
mod tests;
