use super::*;
use aoe_core::TileRect;
use aoe_protocol::{Age, BuildingKind, ConstructionState, ResearchState, TrainingState};

fn producer(id: u32, queue: &[UnitKind]) -> GameplayBuildingState {
    GameplayBuildingState {
        id: EntityId(id),
        player: PlayerId(0),
        kind: BuildingKind::TownCenter,
        footprint: TileRect::from_xywh(id as i32 * 10, 10, 4, 4),
        construction: ConstructionState::Complete,
        progress_permille: 1000,
        hp: 1000,
        max_hp: 1000,
        los: 6,
        food_remaining: None,
        training_queue: queue
            .iter()
            .map(|&kind| TrainingState {
                kind,
                progress_ticks: 0,
                total_ticks: 100,
            })
            .collect(),
        research: None,
        last_hit: None,
        age: Age::Castle,
        gate_open: false,
        rally_point: None,
        farm_reseeds: 0,
        garrison: Vec::new(),
        bridge_span: None,
    }
}

#[test]
fn kingdom_activity_groups_all_owned_work_and_chooses_the_leading_active_producer() {
    let mut buildings = vec![
        producer(7, &[UnitKind::Villager, UnitKind::Villager]),
        producer(2, &[UnitKind::Villager]),
        producer(9, &[UnitKind::Villager]),
        producer(10, &[]),
    ];
    buildings[0].training_queue[0].progress_ticks = 25;
    buildings[1].training_queue[0].progress_ticks = 50;
    buildings[2].player = PlayerId(1);
    buildings[3].research = Some(ResearchState {
        tech: Tech::Loom,
        progress_ticks: 20,
        total_ticks: 100,
    });
    let summary = summarize(&buildings, PlayerId(0));
    assert_eq!(summary.entries.len(), 2);
    let unit = summary.entries[0];
    assert_eq!(
        (unit.count, unit.progress, unit.building),
        (3, 500, EntityId(2))
    );
    assert!(!unit.research);
    let tech = summary.entries[1];
    assert!(tech.research);
    assert_eq!(
        (tech.kind, tech.count, tech.progress, tech.building),
        (Tech::Loom as u8, 1, 200, EntityId(10))
    );
    buildings.reverse();
    assert_eq!(summarize(&buildings, PlayerId(0)), summary);
}

#[test]
fn kingdom_activity_bounds_output_and_ignores_destroyed_or_unfinished_buildings() {
    let mut b = producer(1, &UnitKind::ALL[..8]);
    let summary = summarize(&[b.clone()], PlayerId(0));
    assert_eq!(summary.entries.len(), MAX_ROWS);
    assert_eq!(summary.more, 2);
    b.hp = 0;
    assert!(summarize(&[b.clone()], PlayerId(0)).entries.is_empty());
    b.hp = 1000;
    b.construction = ConstructionState::UnderConstruction;
    assert!(summarize(&[b], PlayerId(0)).entries.is_empty());
    assert!(summarize(&[], PlayerId(0)).entries.is_empty());
}

#[test]
fn kingdom_activity_only_advances_queue_heads_and_clamps_progress() {
    let mut first = producer(1, &[UnitKind::Villager, UnitKind::Archer]);
    first.training_queue[1].progress_ticks = 90;
    let mut second = producer(2, &[UnitKind::Archer]);
    second.training_queue[0].progress_ticks = 10;
    let summary = summarize(&[first, second], PlayerId(0));
    let archers = summary
        .entries
        .iter()
        .find(|e| e.kind == UnitKind::Archer as u8)
        .unwrap();
    assert_eq!(
        (archers.count, archers.progress, archers.building),
        (2, 100, EntityId(2))
    );
    let queued = producer(1, &[UnitKind::Villager, UnitKind::Archer]);
    let starting = producer(2, &[UnitKind::Archer]);
    let summary = summarize(&[queued, starting], PlayerId(0));
    let archers = summary
        .entries
        .iter()
        .find(|e| e.kind == UnitKind::Archer as u8)
        .unwrap();
    assert_eq!(
        archers.building,
        EntityId(2),
        "an active producer wins over waiting work at zero progress"
    );
    let mut full = producer(3, &[UnitKind::Villager]);
    full.training_queue[0].progress_ticks = u64::MAX;
    assert_eq!(
        summarize(&[full.clone()], PlayerId(0)).entries[0].progress,
        1000
    );
    full.training_queue[0].total_ticks = 0;
    assert_eq!(summarize(&[full], PlayerId(0)).entries[0].progress, 0);
}
