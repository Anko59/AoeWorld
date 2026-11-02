use super::*;
use aoe_core::Seed;

#[path = "tests/combat.rs"]
mod combat;

#[tokio::test]
async fn outgoing_validation_and_socket_failures_are_distinguishable() {
    let mut sink = futures_util::sink::drain();
    let invalid = GameplayServerMessage::Projectiles {
        revision: 0,
        tick: aoe_core::Tick(0),
        projectiles: vec![],
    };
    assert!(matches!(
        send_socket(&mut sink, invalid).await,
        Err(SendFailure::Encode(_))
    ));
    let mut broken = Box::pin(futures_util::sink::unfold((), |(), _| async {
        Err::<(), ()>(())
    }));
    assert!(matches!(
        send_socket(
            &mut broken,
            GameplayServerMessage::Error {
                code: 400,
                message: "invalid order".into()
            }
        )
        .await,
        Err(SendFailure::Write)
    ));
}

/// A Snapshot or Tick carrying one invalid entity must not fail whole: the other
/// entities arrive and the failure is counted under its rule.
#[tokio::test]
async fn one_invalid_entity_does_not_freeze_the_stream() {
    use aoe_core::{EntityId, PlayerId, Tick, WorldPosition};
    use aoe_protocol::{GameplayUnitState, UnitKind, UnitTask, decode_gameplay_server};
    let unit = |id: u32| GameplayUnitState {
        id: EntityId(id),
        player: PlayerId(0),
        position: WorldPosition::new(1024, 1024),
        moving: false,
        planning: false,
        facing: 3,
        kind: UnitKind::Villager,
        task: UnitTask::Idle,
        gathering: None,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
        carried: None,
        death_tick: None,
        hunt_target: None,
        death_hunt: None,
        auto_scout: false,
        cargo_count: 0,
        home_castle: None,
        managed_by: None,
        commanded_by: None,
        general: None,
        stance: Default::default(),
    };
    let mut dead_managed = unit(2);
    dead_managed.hp = 0;
    dead_managed.task = UnitTask::Dying;
    dead_managed.managed_by = Some(EntityId(9));
    let mut bad_facing = unit(4);
    bad_facing.facing = 12;
    let (tx, mut sent) = tokio::sync::mpsc::unbounded_channel::<Message>();
    let mut sink = Box::pin(futures_util::sink::unfold(
        tx,
        |tx, m: Message| async move {
            tx.send(m).ok();
            Ok::<_, ()>(tx)
        },
    ));
    for message in [
        GameplayServerMessage::Snapshot {
            revision: 1,
            tick: Tick(1),
            units: vec![unit(1), dead_managed, unit(3), bad_facing],
            players: vec![],
            buildings: vec![],
        },
        GameplayServerMessage::Tick {
            revision: 1,
            tick: Tick(2),
            changed_units: vec![unit(1), dead_managed, unit(3)],
            removals: vec![],
            changed_players: vec![],
            changed_buildings: vec![],
            building_removals: vec![],
        },
    ] {
        send_socket(&mut sink, message)
            .await
            .expect("one bad unit must not fail the delivery");
    }
    for expected in [4, 3] {
        let Some(Message::Binary(bytes)) = sent.recv().await else {
            panic!("a binary frame");
        };
        let ids: Vec<u32> = match decode_gameplay_server(&bytes).unwrap() {
            GameplayServerMessage::Snapshot { units, .. } => units.iter().map(|u| u.id.0).collect(),
            GameplayServerMessage::Tick { changed_units, .. } => {
                changed_units.iter().map(|u| u.id.0).collect()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(ids.len(), expected);
        assert!(ids.contains(&1) && ids.contains(&3));
    }
    let seen = degrade::SEEN.lock().unwrap();
    assert!(seen[&("Snapshot", "unit.managed_by")] >= 1);
    assert!(seen[&("Snapshot", "unit.facing")] >= 1);
    assert!(seen[&("Tick", "unit.managed_by")] >= 1);
}

#[test]
fn repeated_failures_log_at_powers_of_ten_only() {
    let logged: Vec<u64> = (1..=1000)
        .filter(|n| degrade::is_power_of_ten(*n))
        .collect();
    assert_eq!(logged, [1, 10, 100, 1000]);
}

#[test]
fn tokens_are_unpredictable_even_for_the_same_world_and_counter() {
    assert_ne!(make_token(42, 7), make_token(42, 7));
    assert_ne!(make_token(42, 7), make_token(42, 8));
}

#[tokio::test]
async fn a_token_of_another_world_seats_a_page_under_this_worlds_own_token() {
    let retired = GameplayService::new(Seed(1));
    let stale = retired.slot_infos().await[0].token;
    let current = GameplayService::new(Seed(2));
    let (tx, _rx) = mpsc::channel(8);
    let (_, welcome) = current.register(Some(stale), tx).await;
    let GameplayServerMessage::Welcome { resume_token, .. } = welcome else {
        panic!("a page always receives a welcome");
    };
    assert_ne!(resume_token, stale, "the stale credential is never adopted");
    assert!(current.is_human_token(resume_token));
    assert!(!current.is_human_token(stale));
}
