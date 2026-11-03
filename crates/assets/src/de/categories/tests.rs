use super::*;

#[test]
fn categories_partition_every_source_once_without_changing_native_cell_order() {
    let (world, actors) = indices(7, 2..5).unwrap();
    assert_eq!(world, [0, 1, 5, 6]);
    assert_eq!(actors, [2, 3, 4]);
    let mut all = world.into_iter().chain(actors).collect::<Vec<_>>();
    all.sort_unstable();
    assert_eq!(all, (0..7).collect::<Vec<_>>());
    assert!(indices(7, 2..8).is_err());
}

#[test]
fn actor_category_link_is_idempotent_and_never_overwrites_other_content() {
    let root = tempfile::tempdir().unwrap();
    let world = root.path().join("world");
    let actors = root.path().join("actors");
    fs::create_dir(&world).unwrap();
    fs::create_dir(&actors).unwrap();
    attach(&world, &actors).unwrap();
    attach(&world, &actors).unwrap();
    assert_eq!(world.join("actors").canonicalize().unwrap(), actors);
    fs::remove_file(world.join("actors")).unwrap();
    fs::write(world.join("actors"), "keep").unwrap();
    assert!(attach(&world, &actors).is_err());
    assert_eq!(fs::read_to_string(world.join("actors")).unwrap(), "keep");
}
