#![cfg(not(target_arch = "wasm32"))]
use super::super::Followers;
use super::*;

#[test]
fn snapshot_preserves_every_cyclic_sampling_window_and_early_exit() {
    let mut followers = Followers::default();
    for raw in 0..320 {
        followers.insert(
            EntityId(raw),
            TileCoord::new(6 + (raw % 4) as i32, 6 + ((raw / 4) % 4) as i32),
        );
    }
    let mut snapshot = Snapshot::default();
    for tile in [
        TileCoord::new(7, 7),
        TileCoord::new(8, 8),
        TileCoord::new(6, 6),
        TileCoord::new(9, 9),
    ] {
        assert!(snapshot.refresh(&followers, 64, 64));
        for salt in [0, 1, 31, 32, 79, u64::MAX] {
            let except = EntityId(19);
            let mut live = Vec::new();
            followers.any_neighbour(tile, salt, except, |id| {
                live.push(id);
                false
            });
            let mut copied = Vec::new();
            snapshot.any_neighbour(tile, salt, except, |id| {
                copied.push(id);
                false
            });
            assert_eq!(copied, live);
            for chosen in live.iter().step_by(7) {
                let mut live_prefix = Vec::new();
                assert!(followers.any_neighbour(tile, salt, except, |id| {
                    live_prefix.push(id);
                    id == *chosen
                }));
                let mut copy_prefix = Vec::new();
                assert!(snapshot.any_neighbour(tile, salt, except, |id| {
                    copy_prefix.push(id);
                    id == *chosen
                }));
                assert_eq!(copy_prefix, live_prefix);
            }
        }
    }
}

#[test]
fn proposal_snapshot_owns_membership_until_the_ordered_apply_pass() {
    let mut followers = Followers::default();
    let tile = TileCoord::new(8, 8);
    followers.insert(EntityId(1), tile);
    followers.insert(EntityId(2), tile);
    let mut snapshot = Snapshot::default();
    assert!(snapshot.refresh(&followers, 64, 64));
    followers.remove(EntityId(1), tile);
    assert!(snapshot.any_neighbour(tile, 0, EntityId(99), |id| id == EntityId(1)));
    assert!(!followers.any_neighbour(tile, 0, EntityId(99), |id| id == EntityId(1)));
}

#[test]
fn refreshing_reuses_storage_and_removes_vacated_cells() {
    let mut followers = Followers::default();
    let old = TileCoord::new(8, 8);
    let new = TileCoord::new(40, 40);
    followers.insert(EntityId(1), old);
    let mut snapshot = Snapshot::default();
    assert!(snapshot.refresh(&followers, 64, 64));
    let cells = snapshot.cells.as_ptr();
    let pool = snapshot.pool.as_ptr();
    followers.remove(EntityId(1), old);
    followers.insert(EntityId(2), new);
    assert!(snapshot.refresh(&followers, 64, 64));
    assert_eq!(snapshot.cells.as_ptr(), cells);
    assert_eq!(snapshot.pool.as_ptr(), pool);
    assert!(!snapshot.any_neighbour(old, 0, EntityId(99), |_| true));
    assert!(snapshot.any_neighbour(new, 0, EntityId(99), |id| id == EntityId(2)));
    assert_eq!(snapshot.cell_count(), 1);
}

#[test]
fn oversized_maps_decline_before_allocating_a_dense_world_grid() {
    let followers = Followers::default();
    let mut snapshot = Snapshot::default();
    assert!(!snapshot.refresh(&followers, 32_768, 32_768));
    assert!(snapshot.cells.is_empty());
    assert!(snapshot.refresh(&followers, 16_384, 16_384));
    assert_eq!(snapshot.cells.len(), MAX_CELLS);
    assert_eq!(snapshot.cells.len() * std::mem::size_of::<u32>(), 16 << 20);
}
