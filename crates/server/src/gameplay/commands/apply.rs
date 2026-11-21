use super::*;

#[path = "apply/selections.rs"]
mod selections;

/// Accepted overrides apply detachment after the order succeeds. Formation
/// marches and attacks resolve commandership within their simulation paths.
fn commanded_units(
    action: &PendingAction,
) -> Option<(Vec<EntityId>, Option<aoe_core::WorldPosition>)> {
    match action {
        PendingAction::Formation {
            unit_ids,
            destination: march @ None,
            ..
        } => Some((unit_ids.clone(), *march)),
        PendingAction::StopSelection(ids) => Some((ids.clone(), None)),
        PendingAction::AutoScout {
            entity_id,
            enabled: true,
        } => Some((vec![*entity_id], None)),
        PendingAction::GeneralMoveAlone { entity_id, .. } => Some((vec![*entity_id], None)),
        PendingAction::Load { unit_ids, .. } | PendingAction::Hunt { unit_ids, .. } => {
            Some((unit_ids.clone(), None))
        }
        PendingAction::Patrol(entity_id, _)
        | PendingAction::Move { entity_id, .. }
        | PendingAction::Stop { entity_id }
        | PendingAction::Garrison { entity_id, .. } => Some((vec![*entity_id], None)),
        _ => None,
    }
}

pub(in crate::gameplay) fn apply(
    world: &mut aoe_simulation::GameWorld,
    player: aoe_core::PlayerId,
    action: PendingAction,
) -> CommandResult {
    let commanded = commanded_units(&action);
    let result = match action {
        PendingAction::MoveSelection(order) => {
            return selections::move_selection(world, player, order);
        }
        PendingAction::AttackSelection {
            entity_ids,
            target_id,
        } => selections::attack_selection(world, player, &entity_ids, target_id),
        PendingAction::TrainBatch { building_ids, kind } => {
            return selections::apply_each(
                world,
                player,
                building_ids
                    .into_iter()
                    .map(|building_id| PendingAction::Train { building_id, kind }),
            );
        }
        PendingAction::StopSelection(ids) => world.issue_stop_selection(player, &ids),
        PendingAction::Patrol(entity_id, at) => world.issue_patrol(player, entity_id, at),
        PendingAction::VassalDevelopment { vassal_id, enabled } => {
            world.issue_vassal_development(player, vassal_id, enabled)
        }
        PendingAction::VassalFocus { vassal_id, focus } => {
            let focus = match focus {
                aoe_protocol::VassalFocus::Fortify => aoe_simulation::VassalFocus::Fortify,
                aoe_protocol::VassalFocus::Roads => aoe_simulation::VassalFocus::Roads,
            };
            world.issue_vassal_focus(player, vassal_id, focus)
        }
        PendingAction::Stance { unit_ids, stance } => world.issue_stance(player, &unit_ids, stance),
        PendingAction::Garrison {
            entity_id,
            building_id,
        } => world.issue_garrison(player, entity_id, building_id),
        PendingAction::Ungarrison {
            building_id,
            entity_id,
        } => world.issue_ungarrison(player, building_id, entity_id),
        PendingAction::TownBell(building) => world.issue_town_bell(player, building),
        PendingAction::BackToWork(building) => world.issue_back_to_work(player, building),
        PendingAction::Trade(unit, building) => world.issue_trade(player, unit, building),
        PendingAction::Load {
            unit_ids,
            transport_id,
        } => world.issue_load(player, &unit_ids, transport_id),
        PendingAction::Unload {
            transport_id,
            destination,
        } => world.issue_unload(player, transport_id, destination),
        PendingAction::Formation {
            unit_ids,
            formation,
            destination,
            queued,
            attack_move,
        } => match destination {
            Some(destination) => world.issue_player_formation_move(
                player,
                &unit_ids,
                formation,
                destination,
                queued,
                attack_move,
            ),
            None => world.issue_formation(player, &unit_ids, formation),
        },
        PendingAction::AutoScout { entity_id, enabled } => {
            world.issue_auto_scout(player, entity_id, enabled)
        }
        PendingAction::FarmReseed { building_id, add } => {
            world.issue_farm_reseed(player, building_id, add)
        }
        PendingAction::StewardDismiss { steward_id } => {
            world.issue_steward_dismiss(player, steward_id)
        }
        PendingAction::StewardSettings {
            steward_id,
            settings,
        } => world.issue_steward_settings(player, steward_id, settings),
        PendingAction::SetRally {
            building_id,
            target,
        } => world.issue_set_rally(
            player,
            building_id,
            crate::gameplay::economy::native_rally(target),
        ),
        PendingAction::Stop { entity_id } => world.issue_stop(player, entity_id),
        PendingAction::Delete { entity_ids } => world.issue_delete(player, &entity_ids),
        PendingAction::GeneralMoveAlone {
            entity_id,
            destination,
        } => {
            if !world.config().valid_map_position(destination) {
                return CommandResult::RejectedInvalidDestination;
            }
            world.issue_general_move(player, entity_id, destination)
        }
        PendingAction::GeneralTraining { entity_id, enabled } => {
            world.issue_general_training(player, entity_id, enabled)
        }
        PendingAction::GeneralMix { entity_id, mix } => {
            world.issue_general_mix(player, entity_id, mix)
        }
        PendingAction::Hunt {
            unit_ids,
            animal_id,
        } => {
            if !world.animal(animal_id).is_some_and(|a| {
                a.owner == Some(player)
                    || world.visibility(player, a.position.tile_floor())
                        == aoe_simulation::TileVisibility::Visible
            }) {
                return CommandResult::RejectedInvalidTarget;
            }
            world.issue_hunt(player, &unit_ids, animal_id)
        }
        PendingAction::Move {
            entity_id,
            destination,
            queued,
            attack_move,
        } => {
            if world.animal(entity_id).is_some() {
                if queued {
                    return CommandResult::RejectedWrongUnitKind;
                }
                return match world.issue_herd(player, entity_id, destination) {
                    Ok(()) => CommandResult::Accepted,
                    Err(aoe_simulation::EconomyError::NotOwner) => CommandResult::RejectedNotOwner,
                    Err(_) => CommandResult::RejectedInvalidDestination,
                };
            }
            let Some(unit) = world.unit(entity_id) else {
                return CommandResult::RejectedUnknownEntity;
            };
            if unit.player != player {
                return CommandResult::RejectedNotOwner;
            }
            if !world.config().valid_map_position(destination) {
                return CommandResult::RejectedInvalidDestination;
            }
            if unit.kind == aoe_simulation::UnitKind::General {
                world.issue_player_formation_move(
                    player,
                    &[entity_id],
                    aoe_core::Formation::Line,
                    destination,
                    queued,
                    attack_move,
                )
            } else if queued {
                world
                    .issue_queued_leg(entity_id, destination, attack_move)
                    .map(|_| ())
            } else if attack_move {
                world.issue_attack_move(entity_id, destination).map(|_| ())
            } else {
                world
                    .issue_move(entity_id, destination)
                    .map(|_| ())
                    .map_err(aoe_simulation::EconomyError::Movement)
            }
        }
        PendingAction::Gather {
            entity_id,
            resource_id,
        } => world.issue_gather(player, entity_id, resource_id),
        PendingAction::Attack {
            entity_id,
            target_id,
        } => selections::attack_selection(world, player, &[entity_id], target_id),
        PendingAction::CancelTraining { building_id } => {
            world.issue_cancel_training(player, building_id)
        }
        PendingAction::Train { building_id, kind } => world.issue_train(
            player,
            building_id,
            crate::gameplay::economy::native_kind(kind),
        ),
        PendingAction::Build {
            builder_ids,
            kind,
            origin,
            queued,
        } => world
            .issue_build(
                player,
                &builder_ids,
                crate::gameplay::economy::native_building(kind),
                origin,
                queued,
            )
            .map(|_| ()),
        PendingAction::BuildBridge {
            builder_ids,
            from,
            to,
            queued,
        } => world
            .issue_build_bridge(player, &builder_ids, from, to, queued)
            .map(|_| ()),
        PendingAction::BuildWall {
            builder_ids,
            kind,
            from,
            to,
            queued,
        } => world
            .issue_build_wall(
                player,
                &builder_ids,
                crate::gameplay::economy::native_building(kind),
                from,
                to,
                queued,
            )
            .map(|_| ()),
        PendingAction::Construct {
            builder_ids,
            building_id,
        } => world.issue_construct(player, &builder_ids, building_id),
        PendingAction::CancelConstruction { building_id } => {
            world.issue_cancel_construction(player, building_id)
        }
        PendingAction::Farm { entity_id, farm_id } => world.issue_farm(player, entity_id, farm_id),
        PendingAction::Research { building_id, tech } => world.issue_research(
            player,
            building_id,
            crate::gameplay::economy::native_tech(tech),
        ),
        PendingAction::CancelResearch { building_id } => {
            world.issue_cancel_research(player, building_id)
        }
        PendingAction::MarketTrade {
            building_id,
            resource,
            buy,
            lots,
        } => world
            .issue_market_trade(
                player,
                building_id,
                crate::gameplay::economy::native_trade(resource),
                buy,
                lots,
            )
            .map(|_| ()),
    };
    use aoe_simulation::{EconomyError as E, GameWorldError as W};
    if result.is_ok()
        && let Some((ids, march)) = commanded
    {
        world.general_player_order(player, &ids, march);
    }
    match result {
        Ok(()) => CommandResult::Accepted,
        Err(E::UnknownEntity | E::Movement(W::UnknownEntity)) => {
            CommandResult::RejectedUnknownEntity
        }
        Err(E::StewardExists) => CommandResult::RejectedStewardExists,
        Err(E::InvalidStewardSettings) => CommandResult::RejectedInvalidStewardSettings,
        Err(E::GarrisonFull) => CommandResult::RejectedGarrisonFull,
        Err(E::InvalidTarget) => CommandResult::RejectedInvalidTarget,
        Err(E::InsufficientResources) => CommandResult::RejectedInsufficientResources,
        Err(E::NotOwner) => CommandResult::RejectedNotOwner,
        Err(E::WrongUnitKind) => CommandResult::RejectedWrongUnitKind,
        Err(E::InvalidResource) => CommandResult::RejectedInvalidResource,
        Err(E::InsufficientFood) => CommandResult::RejectedInsufficientFood,
        Err(E::PopulationCap) => CommandResult::RejectedPopulationCap,
        Err(E::TrainingQueueFull) => CommandResult::RejectedTrainingQueueFull,
        Err(E::NotBuildable) => CommandResult::RejectedNotBuildable,
        Err(E::RequirementMissing) => CommandResult::RejectedRequirementMissing,
        Err(E::InvalidFootprint) => CommandResult::RejectedInvalidPlacement,
        Err(E::UnexploredFootprint) => CommandResult::RejectedUnexploredPlacement,
        Err(E::UnderConstruction) => CommandResult::RejectedUnderConstruction,
        Err(E::AlreadyComplete) => CommandResult::RejectedAlreadyComplete,
        Err(E::FarmUnavailable) => CommandResult::RejectedFarmUnavailable,
        Err(E::NotResearchable) => CommandResult::RejectedNotResearchable,
        Err(E::AlreadyResearched) => CommandResult::RejectedAlreadyResearched,
        Err(E::ResearchBusy) => CommandResult::RejectedResearchBusy,
        Err(E::RequiresCastleAge) => CommandResult::RejectedRequiresCastleAge,
        Err(E::RequiresImperialAge) => CommandResult::RejectedRequiresImperialAge,
        Err(E::RequiresFeudalAge) => CommandResult::RejectedRequiresFeudalAge,
        Err(E::NoMarket) => CommandResult::RejectedNoMarket,
        Err(E::TechRequirementMissing) => CommandResult::RejectedTechRequirement,
        Err(E::Capacity) => CommandResult::RejectedWorldLimit,
        Err(E::OrderTooLarge) => CommandResult::RejectedOrderTooLarge,
        Err(E::NoAttackPosition | E::Movement(W::Unreachable)) => {
            CommandResult::RejectedUnreachable
        }
        Err(E::Movement(W::PathBudgetExceeded)) => CommandResult::RejectedPathBudgetExceeded,
        Err(E::Movement(W::Environment(_))) => CommandResult::RejectedResourceUnavailable,
        Err(_) => CommandResult::RejectedInvalidDestination,
    }
}
