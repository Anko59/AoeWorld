//! One place where a client's gameplay request enters the match: the socket
//! reader and an in-process seat both dispatch through it.
use crate::gameplay::GameplayService;
use aoe_protocol::{GameplayClientMessage, GameplayServerMessage};

impl GameplayService {
    /// Routes one decoded request of session `session_id` to its queue. A
    /// message a session may not send (`Hello`, `JoinSlot`) is answered with
    /// an error to deliver and closes the conversation.
    pub(crate) async fn dispatch_client_message(
        &self,
        session_id: u64,
        message: GameplayClientMessage,
    ) -> Option<GameplayServerMessage> {
        let service = self;
        match message {
            GameplayClientMessage::DiplomacyOrder { .. }
            | GameplayClientMessage::DiplomacyMessageOrder { .. }
            | GameplayClientMessage::TributeOrder { .. } => {
                super::diplomacy::dispatch(service, session_id, message).await
            }
            GameplayClientMessage::RelicOrder { sequence, action } => {
                service
                    .queue_command(session_id, sequence, super::PendingAction::Relic(action))
                    .await
            }
            GameplayClientMessage::StanceOrder {
                sequence,
                unit_ids,
                stance,
            } => {
                service
                    .queue_stance(session_id, sequence, unit_ids, stance)
                    .await
            }
            GameplayClientMessage::Subscribe {
                revision,
                region,
                unit_zoom,
            } => {
                service
                    .subscribe(session_id, revision, region, unit_zoom)
                    .await
            }
            GameplayClientMessage::MoveSelectionOrder {
                sequence,
                unit_ids,
                formation,
                destination,
                queued,
                attack_move,
            } => {
                service
                    .queue_move_selection(
                        session_id,
                        sequence,
                        crate::gameplay::SelectionMove {
                            unit_ids,
                            formation,
                            destination,
                            queued,
                            attack_move,
                        },
                    )
                    .await
            }
            GameplayClientMessage::AttackSelectionOrder {
                sequence,
                entity_ids,
                target_id,
            } => {
                service
                    .queue_attack_selection(session_id, sequence, entity_ids, target_id)
                    .await
            }
            GameplayClientMessage::GatherSelectionOrder {
                sequence,
                entity_ids,
                resource_id,
            } => {
                service
                    .queue_gather_selection(session_id, sequence, entity_ids, resource_id)
                    .await
            }
            GameplayClientMessage::PatrolSelectionOrder {
                sequence,
                entity_ids,
                destination,
            } => {
                service
                    .queue_patrol_selection(session_id, sequence, entity_ids, destination)
                    .await
            }
            GameplayClientMessage::GarrisonSelectionOrder {
                sequence,
                entity_ids,
                building_id,
            } => {
                service
                    .queue_garrison_selection(session_id, sequence, entity_ids, building_id)
                    .await
            }
            GameplayClientMessage::GeneralMoveAloneSelectionOrder {
                sequence,
                entity_ids,
                destination,
            } => {
                service
                    .queue_general_move_alone_selection(
                        session_id,
                        sequence,
                        entity_ids,
                        destination,
                    )
                    .await
            }
            GameplayClientMessage::TrainBatchOrder {
                sequence,
                kind,
                building_ids,
            } => {
                service
                    .queue_train_batch(session_id, sequence, building_ids, kind)
                    .await
            }
            GameplayClientMessage::FormationOrder {
                sequence,
                unit_ids,
                formation,
                destination,
                queued,
                attack_move,
            } => {
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
            GameplayClientMessage::MoveOrder {
                sequence,
                entity_id,
                destination,
                attack_move,
            } => {
                service
                    .queue_move_order(session_id, sequence, entity_id, destination, attack_move)
                    .await
            }
            GameplayClientMessage::QueueMoveOrder {
                sequence,
                entity_id,
                destination,
                attack_move,
            } => {
                service
                    .queue_waypoint(session_id, sequence, entity_id, destination, attack_move)
                    .await
            }
            GameplayClientMessage::PatrolOrder {
                sequence,
                entity_id,
                destination,
            } => {
                service
                    .queue_patrol(session_id, sequence, entity_id, destination)
                    .await
            }
            GameplayClientMessage::GeneralOrder {
                sequence,
                entity_id,
                command: aoe_protocol::GeneralCommand::MoveAlone { destination },
            } => {
                service
                    .queue_general_move_alone(session_id, sequence, entity_id, destination)
                    .await
            }
            GameplayClientMessage::GeneralOrder {
                sequence,
                entity_id,
                command: aoe_protocol::GeneralCommand::SetTraining { enabled },
            } => {
                service
                    .queue_general_training(session_id, sequence, entity_id, enabled)
                    .await
            }
            GameplayClientMessage::GeneralOrder {
                sequence,
                entity_id,
                command: aoe_protocol::GeneralCommand::SetMix { mix },
            } => {
                service
                    .queue_general_mix(session_id, sequence, entity_id, mix)
                    .await
            }
            GameplayClientMessage::StopOrder { sequence, unit_ids } => {
                service
                    .queue_stop_selection(session_id, sequence, unit_ids)
                    .await
            }
            GameplayClientMessage::AutoScoutOrder {
                sequence,
                unit_ids,
                enabled,
            } => {
                service
                    .queue_auto_scout_selection(session_id, sequence, unit_ids, enabled)
                    .await
            }
            GameplayClientMessage::DeleteOrder {
                sequence,
                entity_ids,
            } => service.queue_delete(session_id, sequence, entity_ids).await,
            GameplayClientMessage::GatherOrder {
                sequence,
                entity_id,
                resource_id,
            } => {
                service
                    .queue_gather(session_id, sequence, entity_id, resource_id)
                    .await
            }
            GameplayClientMessage::CancelTrainingOrder {
                sequence,
                building_id,
            } => {
                service
                    .queue_cancel_training(session_id, sequence, building_id)
                    .await
            }
            GameplayClientMessage::TradeOrder {
                sequence,
                entity_id,
                building_id,
            } => {
                service
                    .queue_trade(session_id, sequence, entity_id, building_id)
                    .await
            }
            GameplayClientMessage::GarrisonOrder {
                sequence,
                entity_id,
                building_id,
            } => {
                service
                    .queue_garrison(session_id, sequence, entity_id, building_id)
                    .await
            }
            GameplayClientMessage::UngarrisonOrder {
                sequence,
                building_id,
                entity_id,
            } => {
                service
                    .queue_ungarrison(session_id, sequence, building_id, entity_id)
                    .await
            }
            GameplayClientMessage::BackToWorkOrder {
                sequence,
                building_id,
            } => {
                service
                    .queue_garrison_control(session_id, sequence, building_id, false)
                    .await
            }
            GameplayClientMessage::TownBellOrder {
                sequence,
                building_id,
            } => {
                service
                    .queue_garrison_control(session_id, sequence, building_id, true)
                    .await
            }
            GameplayClientMessage::SetRallyOrder {
                sequence,
                building_id,
                target,
            } => {
                service
                    .queue_set_rally(session_id, sequence, building_id, target)
                    .await
            }
            GameplayClientMessage::VassalDevelopmentOrder {
                sequence,
                vassal_id,
                enabled,
            } => {
                service
                    .queue_vassal_development(session_id, sequence, vassal_id, enabled)
                    .await
            }
            GameplayClientMessage::VassalFocusOrder {
                sequence,
                vassal_id,
                focus,
            } => {
                service
                    .queue_vassal_focus(session_id, sequence, vassal_id, focus)
                    .await
            }
            GameplayClientMessage::StewardDismissOrder {
                sequence,
                steward_id,
            } => {
                service
                    .queue_steward_dismiss(session_id, sequence, steward_id)
                    .await
            }
            GameplayClientMessage::StewardSettingsOrder {
                sequence,
                steward_id,
                weights,
                cap,
                training,
            } => {
                service
                    .queue_steward_settings(
                        session_id, sequence, steward_id, weights, cap, training,
                    )
                    .await
            }
            GameplayClientMessage::FarmReseedOrder {
                sequence,
                building_id,
                add,
            } => {
                service
                    .queue_farm_reseed(session_id, sequence, building_id, add)
                    .await
            }
            GameplayClientMessage::TrainOrder {
                sequence,
                building_id,
                kind,
            } => {
                service
                    .queue_train(session_id, sequence, building_id, kind)
                    .await
            }
            GameplayClientMessage::AttackOrder {
                sequence,
                entity_id,
                target_id,
            } => {
                service
                    .queue_attack(session_id, sequence, entity_id, target_id)
                    .await
            }
            GameplayClientMessage::BuildOrder {
                sequence,
                builder_ids,
                kind,
                origin,
                queued,
            } => {
                service
                    .queue_build(session_id, sequence, builder_ids, kind, origin, queued)
                    .await
            }
            GameplayClientMessage::BuildWallOrder {
                sequence,
                builder_ids,
                kind,
                from,
                to,
                queued,
            } => {
                service
                    .queue_build_wall(session_id, sequence, builder_ids, kind, (from, to), queued)
                    .await
            }
            GameplayClientMessage::BuildBridgeOrder {
                sequence,
                builder_ids,
                from,
                to,
                queued,
            } => {
                service
                    .queue_build_bridge(session_id, sequence, builder_ids, from, to, queued)
                    .await
            }
            GameplayClientMessage::CancelConstructionOrder {
                sequence,
                building_id,
            } => {
                service
                    .queue_cancel_construction(session_id, sequence, building_id)
                    .await
            }
            GameplayClientMessage::ConstructOrder {
                sequence,
                builder_ids,
                building_id,
            } => {
                service
                    .queue_construct(session_id, sequence, builder_ids, building_id)
                    .await
            }
            GameplayClientMessage::FarmOrder {
                sequence,
                entity_id,
                farm_id,
            } => {
                service
                    .queue_farm(session_id, sequence, entity_id, farm_id)
                    .await
            }
            GameplayClientMessage::ResearchOrder {
                sequence,
                building_id,
                tech,
            } => {
                service
                    .queue_research(session_id, sequence, building_id, tech)
                    .await
            }
            GameplayClientMessage::CancelResearch {
                sequence,
                building_id,
            } => {
                service
                    .queue_cancel_research(session_id, sequence, building_id)
                    .await
            }
            GameplayClientMessage::HuntOrder {
                sequence,
                unit_ids,
                animal_id,
            } => {
                service
                    .queue_hunt(session_id, sequence, unit_ids, animal_id)
                    .await
            }
            GameplayClientMessage::MarketBuy {
                sequence,
                building_id,
                resource,
            } => {
                service
                    .queue_market_buy(session_id, sequence, building_id, resource)
                    .await
            }
            GameplayClientMessage::MarketSell {
                sequence,
                building_id,
                resource,
            } => {
                service
                    .queue_market_sell(session_id, sequence, building_id, resource)
                    .await
            }
            GameplayClientMessage::MarketTradeOrder {
                sequence,
                building_id,
                resource,
                buy,
                lots,
            } => {
                service
                    .queue_market_trade(session_id, sequence, building_id, resource, buy, lots)
                    .await
            }
            GameplayClientMessage::LoadOrder {
                sequence,
                unit_ids,
                transport_id,
            } => {
                service
                    .queue_load(session_id, sequence, unit_ids, transport_id)
                    .await
            }
            GameplayClientMessage::UnloadOrder {
                sequence,
                transport_id,
                destination,
            } => {
                service
                    .queue_unload(session_id, sequence, transport_id, destination)
                    .await
            }
            GameplayClientMessage::Resync { revision } => {
                service.resync(session_id, revision).await
            }
            GameplayClientMessage::Hello { .. } | GameplayClientMessage::JoinSlot { .. } => {
                return Some(GameplayServerMessage::Error {
                    code: 400,
                    message: "invalid gameplay request".to_owned(),
                });
            }
        }
        None
    }
}
