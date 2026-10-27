//! Socket reads stay live independently of backpressured writes.
use super::*;
use futures_util::stream::SplitStream;

pub(super) async fn read_requests(
    service: &GameplayService,
    session_id: u64,
    stream: &mut SplitStream<WebSocket>,
) -> Option<GameplayServerMessage> {
    while let Some(Ok(frame)) = stream.next().await {
        let bytes = match frame {
            Message::Binary(bytes) => bytes,
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => break,
            _ => {
                return Some(GameplayServerMessage::Error {
                    code: 400,
                    message: "expected a binary gameplay request".to_owned(),
                });
            }
        };
        match decode_gameplay_client(&bytes) {
            Ok(GameplayClientMessage::StanceOrder {
                sequence,
                unit_ids,
                stance,
            }) => {
                service
                    .queue_stance(session_id, sequence, unit_ids, stance)
                    .await
            }
            Ok(GameplayClientMessage::Subscribe { revision, region }) => {
                service.subscribe(session_id, revision, region).await
            }
            Ok(GameplayClientMessage::FormationOrder {
                sequence,
                unit_ids,
                formation,
                destination,
                queued,
                attack_move,
            }) => {
                service
                    .queue_formation(
                        session_id,
                        sequence,
                        unit_ids,
                        formation,
                        destination,
                        queued,
                        attack_move,
                    )
                    .await
            }
            Ok(GameplayClientMessage::MoveOrder {
                sequence,
                entity_id,
                destination,
            }) => {
                service
                    .queue_move(session_id, sequence, entity_id, destination)
                    .await
            }
            Ok(GameplayClientMessage::QueueMoveOrder {
                sequence,
                entity_id,
                destination,
            }) => {
                service
                    .queue_waypoint(session_id, sequence, entity_id, destination)
                    .await
            }
            Ok(GameplayClientMessage::StopOrder {
                sequence,
                entity_id,
            }) => service.queue_stop(session_id, sequence, entity_id).await,
            Ok(GameplayClientMessage::GatherOrder {
                sequence,
                entity_id,
                resource_id,
            }) => {
                service
                    .queue_gather(session_id, sequence, entity_id, resource_id)
                    .await
            }
            Ok(GameplayClientMessage::CancelTrainingOrder {
                sequence,
                building_id,
            }) => {
                service
                    .queue_cancel_training(session_id, sequence, building_id)
                    .await
            }
            Ok(GameplayClientMessage::GarrisonOrder {
                sequence,
                entity_id,
                building_id,
            }) => {
                service
                    .queue_garrison(session_id, sequence, entity_id, building_id)
                    .await
            }
            Ok(GameplayClientMessage::UngarrisonOrder {
                sequence,
                building_id,
                entity_id,
            }) => {
                service
                    .queue_ungarrison(session_id, sequence, building_id, entity_id)
                    .await
            }
            Ok(GameplayClientMessage::AutoScoutOrder {
                sequence,
                entity_id,
                enabled,
            }) => {
                service
                    .queue_auto_scout(session_id, sequence, entity_id, enabled)
                    .await
            }
            Ok(GameplayClientMessage::SetRallyOrder {
                sequence,
                building_id,
                target,
            }) => {
                service
                    .queue_set_rally(session_id, sequence, building_id, target)
                    .await
            }
            Ok(GameplayClientMessage::FarmReseedOrder {
                sequence,
                building_id,
                add,
            }) => {
                service
                    .queue_farm_reseed(session_id, sequence, building_id, add)
                    .await
            }
            Ok(GameplayClientMessage::TrainOrder {
                sequence,
                building_id,
                kind,
            }) => {
                service
                    .queue_train(session_id, sequence, building_id, kind)
                    .await
            }
            Ok(GameplayClientMessage::AttackOrder {
                sequence,
                entity_id,
                target_id,
            }) => {
                service
                    .queue_attack(session_id, sequence, entity_id, target_id)
                    .await
            }
            Ok(GameplayClientMessage::BuildOrder {
                sequence,
                builder_ids,
                kind,
                origin,
                queued,
            }) => {
                service
                    .queue_build(session_id, sequence, builder_ids, kind, origin, queued)
                    .await
            }
            Ok(GameplayClientMessage::BuildWallOrder {
                sequence,
                builder_ids,
                from,
                to,
                queued,
            }) => {
                service
                    .queue_build_wall(session_id, sequence, builder_ids, from, to, queued)
                    .await
            }
            Ok(GameplayClientMessage::BuildBridgeOrder {
                sequence,
                builder_ids,
                from,
                to,
                queued,
            }) => {
                service
                    .queue_build_bridge(session_id, sequence, builder_ids, from, to, queued)
                    .await
            }
            Ok(GameplayClientMessage::CancelConstructionOrder {
                sequence,
                building_id,
            }) => {
                service
                    .queue_cancel_construction(session_id, sequence, building_id)
                    .await
            }
            Ok(GameplayClientMessage::ConstructOrder {
                sequence,
                builder_ids,
                building_id,
            }) => {
                service
                    .queue_construct(session_id, sequence, builder_ids, building_id)
                    .await
            }
            Ok(GameplayClientMessage::FarmOrder {
                sequence,
                entity_id,
                farm_id,
            }) => {
                service
                    .queue_farm(session_id, sequence, entity_id, farm_id)
                    .await
            }
            Ok(GameplayClientMessage::ResearchOrder {
                sequence,
                building_id,
                tech,
            }) => {
                service
                    .queue_research(session_id, sequence, building_id, tech)
                    .await
            }
            Ok(GameplayClientMessage::CancelResearch {
                sequence,
                building_id,
            }) => {
                service
                    .queue_cancel_research(session_id, sequence, building_id)
                    .await
            }
            Ok(GameplayClientMessage::HuntOrder {
                sequence,
                unit_ids,
                animal_id,
            }) => {
                service
                    .queue_hunt(session_id, sequence, unit_ids, animal_id)
                    .await
            }
            Ok(GameplayClientMessage::MarketBuy {
                sequence,
                building_id,
                resource,
            }) => {
                service
                    .queue_market_buy(session_id, sequence, building_id, resource)
                    .await
            }
            Ok(GameplayClientMessage::MarketSell {
                sequence,
                building_id,
                resource,
            }) => {
                service
                    .queue_market_sell(session_id, sequence, building_id, resource)
                    .await
            }
            Ok(GameplayClientMessage::LoadOrder {
                sequence,
                unit_ids,
                transport_id,
            }) => {
                service
                    .queue_load(session_id, sequence, unit_ids, transport_id)
                    .await
            }
            Ok(GameplayClientMessage::UnloadOrder {
                sequence,
                transport_id,
                destination,
            }) => {
                service
                    .queue_unload(session_id, sequence, transport_id, destination)
                    .await
            }
            Ok(GameplayClientMessage::Resync { revision }) => {
                service.resync(session_id, revision).await
            }
            Ok(GameplayClientMessage::Hello { .. } | GameplayClientMessage::JoinSlot { .. })
            | Err(_) => {
                return Some(GameplayServerMessage::Error {
                    code: 400,
                    message: "invalid gameplay request".to_owned(),
                });
            }
        }
    }
    None
}
