use crate::playground::game::map::feedback::*;
use aoe_core::TileCoord;
use aoe_map::{ObjectKind, ResourceKind, ResourceNode};
use aoe_protocol::{CommandResult, ResourceAmount, ResourceState};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn unresolved_terrain_failures_drop_oldest_at_sixty_four_and_remove_on_recovery() {
    let mut failures = TerrainFailures::default();
    for x in 0..64 {
        failures.chunks.insert((x, 0));
    }
    failures.chunks.insert((0, 0)); // Retrying the oldest does not renew its age.
    failures.chunks.insert((64, 0));
    assert_eq!(failures.chunks.len(), 64);
    assert!(!failures.chunks.contains(&(0, 0)));
    assert!(failures.chunks.contains(&(64, 0)));
    assert!(failures.chunks.remove(&(64, 0)));
    assert!(!failures.chunks.remove(&(64, 0)));
    failures.chunks.insert((65, 0));
    assert_eq!(failures.chunks.len(), 64);
    assert!(failures.chunks.contains(&(1, 0)));
}

#[wasm_bindgen_test]
fn movement_labels_name_resources_and_leave_base_terrain_intact() {
    assert_eq!(
        movement_label(true, Some(ObjectKind::Tree)),
        "tree, blocked"
    );
    assert_eq!(
        movement_label(false, Some(ObjectKind::GoldDeposit)),
        "gold deposit, blocked"
    );
    assert_eq!(
        movement_label(true, Some(ObjectKind::StoneDeposit)),
        "stone deposit, blocked"
    );
    assert_eq!(
        movement_label(true, Some(ObjectKind::ForageBush)),
        "forage bush, blocked"
    );
    assert_eq!(movement_label(true, None), "passable");
    assert_eq!(movement_label(false, None), "blocked");
}

#[wasm_bindgen_test]
fn depletion_and_initial_amount_control_resource_feedback() {
    let mut state = crate::resource_state::ResourceStateCache::default();
    let node = ResourceNode {
        id: 2,
        tile: TileCoord::new(1, 0),
        kind: ResourceKind::Wood,
        object: ObjectKind::Tree,
        initial_amount: 100,
        visual_variant: 0,
    };
    assert_eq!(live_blocker(&node, &state), None);
    assert!(state.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![],
    }));
    assert_eq!(live_blocker(&node, &state), Some(ObjectKind::Tree));
    assert_eq!(
        live_blocker(
            &ResourceNode {
                initial_amount: 0,
                ..node
            },
            &state
        ),
        None
    );
    assert!(state.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(0),
        revision: 1,
        changes: vec![ResourceAmount {
            id: 2,
            remaining: 0
        }],
    }));
    assert_eq!(live_blocker(&node, &state), None);
}

#[wasm_bindgen_test]
fn rejection_sentences_only_explain_a_resource_for_invalid_destinations() {
    use CommandResult::*;
    assert_eq!(order_message(Accepted, Some(ObjectKind::Tree)), "connected");
    assert_eq!(
        order_message(RejectedInvalidDestination, Some(ObjectKind::Tree)),
        "Cannot move there: a tree is in the way."
    );
    for reason in [
        RejectedNotController,
        RejectedUnknownEntity,
        RejectedSequence,
        RejectedRateLimited,
        RejectedQueueFull,
        RejectedInvalidDestination,
        RejectedUnreachable,
        RejectedPathBudgetExceeded,
    ] {
        let label = order_message(reason, None);
        assert!(label.starts_with(char::is_uppercase));
        assert!(label.ends_with('.'));
        assert!(!label.contains('\n'));
        assert!(!label.contains("Rejected"));
        if reason != RejectedInvalidDestination {
            assert_eq!(order_message(reason, Some(ObjectKind::Tree)), label);
        }
    }
}

#[wasm_bindgen_test]
fn terrain_warning_waits_for_each_affected_chunk_and_bounds() {
    let mut failures = TerrainFailures::default();
    assert_eq!(failures.message(), None);
    failures.chunks.insert((1, 1));
    failures.chunks.insert((2, 1));
    let message = failures.message().unwrap();
    assert_eq!(message, "Some terrain could not be loaded; retrying.");
    assert!(message.len() <= 100);
    assert!(!message.contains('\n'));
    failures.chunks.remove(&(3, 1));
    assert_eq!(failures.message(), Some(message));
    failures.chunks.remove(&(1, 1));
    assert_eq!(failures.message(), Some(message));
    failures.bounds = true;
    failures.chunks.remove(&(2, 1));
    assert_eq!(failures.message(), Some(message));
    failures.bounds = false;
    assert_eq!(failures.message(), None);
}

#[wasm_bindgen_test]
fn acknowledgements_use_their_own_target_context_and_keep_a_bounded_history() {
    use CommandResult::RejectedInvalidDestination;
    let mut orders = OrderFeedback::default();
    orders.record(1, Some(ObjectKind::Tree));
    orders.record(2, None);
    assert_eq!(
        orders.acknowledge(2, RejectedInvalidDestination),
        order_message(RejectedInvalidDestination, None)
    );
    assert_eq!(
        orders.acknowledge(1, RejectedInvalidDestination),
        "Cannot move there: a tree is in the way."
    );
    assert_eq!(
        orders.acknowledge(1, RejectedInvalidDestination),
        order_message(RejectedInvalidDestination, None)
    );
    for sequence in 3..=3 + aoe_protocol::MAX_ACK_HISTORY as u64 {
        orders.record(sequence, Some(ObjectKind::Tree));
    }
    assert_eq!(
        orders.acknowledge(3, RejectedInvalidDestination),
        order_message(RejectedInvalidDestination, None)
    );
    assert_eq!(
        orders.acknowledge(4, RejectedInvalidDestination),
        "Cannot move there: a tree is in the way."
    );
    orders.clear();
    assert_eq!(
        orders.acknowledge(5, RejectedInvalidDestination),
        order_message(RejectedInvalidDestination, None)
    );
}

#[wasm_bindgen_test]
fn economy_rejections_remain_clear_in_the_existing_client() {
    for (result, phrase) in [
        (CommandResult::RejectedNotOwner, "another player"),
        (CommandResult::RejectedWrongUnitKind, "unit kind"),
        (CommandResult::RejectedInvalidResource, "exhausted"),
        (CommandResult::RejectedInsufficientFood, "food"),
        (CommandResult::RejectedPopulationCap, "population"),
        (CommandResult::RejectedTrainingQueueFull, "training queue"),
        (CommandResult::RejectedResourceUnavailable, "unavailable"),
        (CommandResult::RejectedWorldLimit, "world"),
        (CommandResult::RejectedOrderTooLarge, "too large"),
    ] {
        assert!(order_message(result, None).contains(phrase));
    }
}

#[wasm_bindgen_test]
fn only_a_full_order_queue_says_the_queue_is_full() {
    for result in [
        CommandResult::RejectedWorldLimit,
        CommandResult::RejectedOrderTooLarge,
    ] {
        assert!(!order_message(result, None).contains("queue"));
    }
}
