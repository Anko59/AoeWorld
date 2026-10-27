use super::*;
pub(in crate::gameplay) fn apply(
    world: &mut aoe_simulation::GameWorld,
    player: aoe_core::PlayerId,
    action: PendingAction,
) -> CommandResult {
    let result = match action {
        PendingAction::Garrison {
            entity_id,
            building_id,
        } => world.issue_garrison(player, entity_id, building_id),
        PendingAction::Ungarrison {
            building_id,
            entity_id,
        } => world.issue_ungarrison(player, building_id, entity_id),
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
            Some(destination) => world.issue_formation_move(
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
        PendingAction::SetRally {
            building_id,
            target,
        } => world.issue_set_rally(
            player,
            building_id,
            crate::gameplay::economy::native_rally(target),
        ),
        PendingAction::Stop { entity_id } => world.issue_stop(player, entity_id),
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
            if queued {
                world.issue_queued_move(entity_id, destination).map(|_| ())
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
        } => {
            let visible = world.unit_visible_to(player, target_id)
                || world.animal(target_id).is_some_and(|animal| {
                    animal.owner == Some(player)
                        || world.visibility(player, animal.position.tile_floor())
                            == aoe_simulation::TileVisibility::Visible
                })
                || world
                    .building(target_id)
                    .is_some_and(|b| world.building_visible_to(player, b.footprint));
            if !visible {
                return CommandResult::RejectedInvalidTarget;
            }
            world.issue_attack(player, entity_id, target_id)
        }
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
            from,
            to,
            queued,
        } => world
            .issue_build_wall(player, &builder_ids, from, to, queued)
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
        PendingAction::MarketBuy {
            building_id,
            resource,
        } => world.issue_market_buy(
            player,
            building_id,
            crate::gameplay::economy::native_trade(resource),
        ),
        PendingAction::MarketSell {
            building_id,
            resource,
        } => world.issue_market_sell(
            player,
            building_id,
            crate::gameplay::economy::native_trade(resource),
        ),
    };
    use aoe_simulation::{EconomyError as E, GameWorldError as W};
    match result {
        Ok(()) => CommandResult::Accepted,
        Err(E::UnknownEntity | E::Movement(W::UnknownEntity)) => {
            CommandResult::RejectedUnknownEntity
        }
        Err(E::VassalExists) => CommandResult::RejectedVassalExists,
        Err(E::VassalHome) => CommandResult::RejectedVassalHome,
        Err(E::Movement(W::VassalLeash)) => CommandResult::RejectedVassalLeash,
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
