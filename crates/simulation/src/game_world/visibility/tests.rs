use super::{PlayerVision, TileVisibility};
use crate::{GameBuilding, GameWorld};
use aoe_core::{EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig};

#[test]
fn empty_visibility_chunks_are_retired_but_exploration_persists() {
    let mut vision = PlayerVision::default();
    // Include negative coordinates and chunk boundaries in the storage contract.
    for tile in [
        TileCoord::new(31, 31),
        TileCoord::new(32, 32),
        TileCoord::new(-1, -1),
    ] {
        assert!(vision.raise_visible(tile));
        vision.set_explored(tile);
        assert_eq!(vision.state(tile), TileVisibility::Visible);
        assert!(vision.lower_visible(tile));
        assert_eq!(vision.state(tile), TileVisibility::Explored);
        assert_eq!(vision.visible_tiles, 0);
        assert!(
            vision.visible.is_empty(),
            "empty counter chunks must be freed"
        );
        assert!(
            !vision.lower_visible(tile),
            "retired counts cannot underflow"
        );
    }
    assert_eq!(vision.explored.len(), 3);
}

#[test]
fn overlapping_observers_keep_a_chunk_until_its_last_visible_tile_leaves() {
    let mut vision = PlayerVision::default();
    let overlap = TileCoord::new(12, 12);
    let neighbor = TileCoord::new(13, 12);
    assert!(vision.raise_visible(overlap));
    assert!(!vision.raise_visible(overlap));
    assert!(vision.raise_visible(neighbor));
    vision.set_explored(overlap);
    vision.set_explored(neighbor);

    assert!(!vision.lower_visible(overlap));
    assert_eq!(vision.state(overlap), TileVisibility::Visible);
    assert_eq!(vision.visible_tiles, 2);
    assert_eq!(vision.visible.len(), 1);
    assert!(vision.lower_visible(overlap));
    assert_eq!(vision.state(overlap), TileVisibility::Explored);
    assert_eq!(vision.state(neighbor), TileVisibility::Visible);
    assert_eq!(vision.visible_tiles, 1);
    assert_eq!(vision.visible.len(), 1);

    assert!(vision.lower_visible(neighbor));
    assert_eq!(vision.visible_tiles, 0);
    assert!(
        vision.visible.is_empty(),
        "the final visible tile releases its chunk"
    );
}

#[test]
fn repeated_16000_axis_exploration_sweeps_keep_active_chunk_allocations_flat() {
    const AXIS: i32 = 16_000;
    const CHUNK_SIDE: i32 = 32;
    const CHUNKS: usize = (AXIS / CHUNK_SIDE * (AXIS / CHUNK_SIDE)) as usize;
    let mut vision = PlayerVision::default();
    let mut peaks = Vec::new();
    // Touch every chunk of the owner's map three times, using one tile per
    // chunk to isolate retained counter allocations from LOS/pathfinding cost.
    // Exploration is durable; only currently visible counter arrays are live.
    for sweep in 0..3 {
        let mut peak_bytes = 0;
        for y in (0..AXIS).step_by(CHUNK_SIDE as usize) {
            for x in (0..AXIS).step_by(CHUNK_SIDE as usize) {
                let tile = TileCoord::new(x, y);
                assert!(vision.raise_visible(tile));
                vision.set_explored(tile);
                let bytes = vision.visible.len() * 32 * 32 * std::mem::size_of::<u32>();
                peak_bytes = peak_bytes.max(bytes);
                assert!(
                    bytes <= 4096,
                    "sweep {sweep}, ({x}, {y}): {bytes} live counter bytes"
                );
                assert!(vision.lower_visible(tile));
                assert_eq!(vision.state(tile), TileVisibility::Explored);
            }
        }
        assert_eq!(vision.explored.len(), CHUNKS);
        assert!(vision.visible.is_empty());
        assert_eq!(vision.visible_tiles, 0);
        peaks.push(peak_bytes);
        eprintln!(
            "sweep {sweep}: {CHUNKS} explored chunks, {peak_bytes} peak counter bytes, 0 retained counter bytes"
        );
    }
    assert_eq!(peaks, vec![4096; 3]);
}

fn remembered_building() -> GameBuilding {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = world
        .place_town_center(PlayerId(1), TileCoord::new(64, 64))
        .unwrap();
    world
        .buildings()
        .find(|building| building.id == id)
        .unwrap()
        .clone()
}

#[test]
fn ghost_budget_evicts_oldest_haunt_and_every_related_index() {
    const BUDGET: usize = 4096;
    let template = remembered_building();
    let mut vision = PlayerVision::default();
    // Descending ids distinguish insertion order from BTreeMap key order.
    let oldest = EntityId(20_000);
    for index in 0..=BUDGET {
        let id = EntityId(20_000 - index as u32);
        let footprint =
            TileRect::from_xywh((index % 100) as i32 * 8, (index / 100) as i32 * 8, 4, 4);
        let mut building = template.clone();
        building.id = id;
        building.footprint = footprint;
        vision.remembered.insert(id, building);
        vision.haunt(id, footprint);
        assert!(
            vision.ghost_sites.len() <= BUDGET,
            "ghost storage needs a fixed budget"
        );
        assert!(vision.ghost_tiles.len() <= BUDGET * 16);
        assert!(
            vision.remembered.contains_key(&id),
            "newest ghost must survive"
        );
    }
    assert_eq!(vision.ghost_sites.len(), BUDGET);
    assert_eq!(vision.remembered.len(), BUDGET);
    assert!(!vision.remembered.contains_key(&oldest));
    assert!(!vision.ghost_sites.contains_key(&oldest));
    assert!(
        vision.changed.contains(&oldest),
        "consumers must forget the evicted ghost"
    );
    assert!(
        !vision
            .ghost_tiles
            .values()
            .flatten()
            .any(|id| *id == oldest)
    );
    assert_eq!(
        vision.ghost_tiles.values().map(Vec::len).sum::<usize>(),
        BUDGET * 16
    );
}

#[test]
fn repeated_haunt_replaces_its_footprint_without_duplicate_or_stale_tile_entries() {
    let building = remembered_building();
    let id = building.id;
    let old = building.footprint;
    let mut vision = PlayerVision::default();
    vision.remembered.insert(id, building);
    for _ in 0..3 {
        vision.haunt(id, old);
    }
    assert_eq!(vision.ghost_sites.len(), 1);
    assert!(vision.ghost_tiles.values().all(|ids| ids == &[id]));

    let new = TileRect::from_xywh(100, 100, 4, 4);
    vision.remembered.get_mut(&id).unwrap().footprint = new;
    vision.haunt(id, new);
    assert_eq!(vision.ghost_sites[&id], new);
    assert!(vision.remembered.contains_key(&id));
    assert_eq!(vision.ghost_tiles.len(), 16);
    assert!(vision.ghost_tiles.keys().all(|tile| new.contains(*tile)));
    assert!(vision.ghost_tiles.values().all(|ids| ids == &[id]));
    vision.exorcise(id);
    assert!(vision.remembered.is_empty());
    assert!(vision.changed.contains(&id));
    assert!(vision.ghost_sites.is_empty());
    assert!(vision.ghost_tiles.is_empty());
}

#[test]
fn ghost_eviction_order_is_part_of_replay_state() {
    let template = remembered_building();
    let make = |ids: [u32; 2]| {
        let mut vision = super::Vision::default();
        let player = vision.players.entry(PlayerId(0)).or_default();
        for number in ids {
            let mut building = template.clone();
            building.id = EntityId(number);
            player.remembered.insert(building.id, building.clone());
            player.haunt(building.id, building.footprint);
        }
        let mut hash = blake3::Hasher::new();
        vision.update_hash(&mut hash);
        hash.finalize()
    };
    assert_ne!(
        make([1, 2]),
        make([2, 1]),
        "different future eviction decisions need different replay hashes"
    );
}
