use super::*;

#[test]
fn sets_reads_and_releases_sparse_pages() {
    let mut grid = Grid::new(0);
    assert_eq!(grid.get(-1, 3), 0);
    grid.set(3, 4, 7);
    grid.set(1_000, 2, 9);
    grid.set(-5, 2, 1);
    assert_eq!(
        (grid.get(3, 4), grid.get(1_000, 2), grid.get(-5, 2)),
        (7, 9, 0)
    );
    assert_eq!(grid.get(4, 4), 0);
    grid.set(3, 4, 0);
    assert_eq!(grid.pages.len() - grid.free.len(), 1);
    let mut entries: Vec<_> = grid.entries().collect();
    entries.sort_unstable();
    assert_eq!(entries, [(1_000, 2, 9)]);
    grid.set(5, 70_000, 3);
    assert_eq!((grid.get(5, 70_000), grid.get(1_000, 2)), (3, 9));
}

#[test]
fn far_corner_grid_memory_is_bounded_on_a_16384_tile_map() {
    // Exercise both the fine collision grid and the half-resolution land counts.
    for scale in [1, 2] {
        let mut grid = Grid::new(0);
        let far = 16_383 / scale;
        grid.set(far, far, 7);
        let bytes = std::mem::size_of_val(&grid)
            + grid.directory.capacity() * std::mem::size_of::<u32>()
            + grid.pages.capacity() * std::mem::size_of::<Page>()
            + grid.free.capacity() * std::mem::size_of::<u32>();
        eprintln!("far-corner collision grid scale {scale}: {bytes} bytes");
        assert_eq!(bytes, 1148, "grid allocation regression (64-bit target)");
        for (x, y) in [
            (far - 16, far),
            (far, far - 16),
            (far + 16, far),
            (far, far + 16),
        ] {
            grid.set(x, y, 9);
            assert_eq!(grid.get(far, far), 7);
            assert_eq!(grid.get(x, y), 9);
        }
        assert_eq!(grid.get(0, 0), 0);
        grid.set(far, far, 0);
        assert_eq!(grid.get(far, far), 0);
        assert_eq!(grid.entries().count(), 4);
        grid.set(far, far, 11);
        assert_eq!(grid.get(far, far), 11);
        assert_eq!(grid.entries().count(), 5);
    }
}

#[test]
fn successive_left_and_up_page_steps_use_geometric_slack() {
    let mut grid = Grid::new(0);
    for step in 0..16 {
        grid.set(16_383 - step * 16, 16_383 - step * 16, step as u32 + 1);
    }
    assert!(grid.directory.capacity() <= 32 * 32);
    let mut entries: Vec<_> = grid.entries().collect();
    entries.sort_unstable();
    let expected: Vec<_> = (0..16)
        .rev()
        .map(|step| (16_383 - step * 16, 16_383 - step * 16, step as u32 + 1))
        .collect();
    assert_eq!(entries, expected);
}
