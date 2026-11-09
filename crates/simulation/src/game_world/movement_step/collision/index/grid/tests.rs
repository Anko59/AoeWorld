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
