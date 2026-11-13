//! A start kit must never box a unit in: no free tile it cuts off, no lone
//! blocker peppering open ground. Dense woodland (the playtests' "villagers
//! stuck in tree patches") is where irregular clusters used to do both.
use super::*;

fn dense_woodland(seed: u64) -> GameWorld {
    let mut world = super::barren();
    let center = TileCoord::new(2050, 2050);
    let Terrain::Map { overlay, .. } = &mut world.terrain else {
        unreachable!()
    };
    // Roughly a third of the land holds natural trees, hashed per seed, with the
    // Town Center clearing kept free.
    for y in -34_i32..=34 {
        for x in -34_i32..=34 {
            let hash = (x as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add((y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
                .wrapping_add(seed.wrapping_mul(0x1656_67B1_9E37_79F9));
            if x.abs().max(y.abs()) > 4 && (hash >> 33) % 100 < 34 {
                overlay
                    .add_footprint(aoe_core::TileRect::from_xywh(
                        center.x + x,
                        center.y + y,
                        1,
                        1,
                    ))
                    .unwrap();
            }
        }
    }
    world
}

/// Tiles 4-connected to `origin` within 18 tiles, where `open` says which tiles
/// count as walkable.
fn reach(
    world: &GameWorld,
    origin: TileCoord,
    center: TileCoord,
    open: impl Fn(TileCoord) -> bool,
) -> BTreeSet<TileCoord> {
    let mut seen = BTreeSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some(tile) = queue.pop_front() {
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if next.x.abs_diff(center.x) <= 18
                && next.y.abs_diff(center.y) <= 18
                && open(next)
                && world.terrain().crossable(tile, next, world.config())
                && seen.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    seen
}

#[test]
fn start_clusters_cut_off_no_free_tile_and_leave_no_lone_blocker() {
    let mut checked = 0;
    for seed in 0..24_u64 {
        let mut world = dense_woodland(seed);
        world.config.seed = Seed(seed);
        let start = TileCoord::new(2048, 2048);
        if world.initialize_village(PlayerId(0), start).is_err() {
            continue;
        }
        checked += 1;
        let footprint = world.buildings().next().unwrap().footprint;
        let center = TileCoord::new(footprint.min.x + 2, footprint.min.y + 2);
        let origin = world
            .units()
            .find(|u| u.kind == crate::UnitKind::Villager)
            .unwrap()
            .position
            .tile_floor();
        let kit: BTreeSet<TileCoord> = world.starting_resources().iter().map(|n| n.tile).collect();
        // Free ground the natural trees alone leave reachable, against what the
        // kit leaves reachable: nothing may be newly cut off.
        let natural = reach(&world, origin, center, |t| {
            world.tile_passable(t) || kit.contains(&t)
        });
        let actual = reach(&world, origin, center, |t| world.tile_passable(t));
        let cut_off: Vec<_> = natural
            .iter()
            .filter(|t| !kit.contains(t) && !actual.contains(t))
            .collect();
        assert!(cut_off.is_empty(), "seed {seed}: kit cuts off {cut_off:?}");
        for tile in &kit {
            let touches = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    (dx, dy) != (0, 0) && (!world.tile_passable(next) || footprint.contains(next))
                })
            });
            assert!(touches, "seed {seed}: lone start blocker at {tile:?}");
        }
    }
    assert!(checked >= 12, "only {checked} woodland starts were placed");
}
