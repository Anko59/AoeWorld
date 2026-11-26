use super::raster::raster_changed;
use super::*;
use std::collections::BTreeSet;
#[test]
fn discs_union_clip_and_resolve_overlaps_without_double_counting() {
    let sources = [
        (PlayerId(1), TileCoord::new(0, 0), 2),
        (PlayerId(0), TileCoord::new(2, 0), 1),
    ];
    let a = raster(&sources, 4, 4);
    let b = raster(&[sources[1], sources[0], sources[0]], 4, 4);
    assert_eq!(a, b);
    let tiles: i32 = a.values().flatten().map(|s| s.x1 - s.x0).sum();
    assert_eq!(tiles, 8);
    assert_eq!(
        a[&0],
        vec![
            HoldingSpan {
                player: PlayerId(1),
                y: 0,
                x0: 0,
                x1: 1
            },
            HoldingSpan {
                player: PlayerId(0),
                y: 0,
                x0: 1,
                x1: 4
            }
        ]
    );
}

#[test]
fn nearest_source_keeps_a_high_id_town_center_inside_a_low_id_domain() {
    let rows = raster(
        &[
            (PlayerId(0), TileCoord::new(50, 50), 100),
            (PlayerId(1), TileCoord::new(90, 50), 12),
        ],
        256,
        256,
    );
    assert_eq!(
        rows[&50]
            .iter()
            .find(|s| s.x0 <= 90 && 90 < s.x1)
            .unwrap()
            .player,
        PlayerId(1)
    );
    assert_eq!(
        rows[&50]
            .iter()
            .find(|s| s.x0 <= 50 && 50 < s.x1)
            .unwrap()
            .player,
        PlayerId(0)
    );
}

#[test]
fn clipped_chunk_fragments_coalesce_before_any_wire_limit() {
    let mut spans = (0..7)
        .map(|n| HoldingSpan {
            player: PlayerId(0),
            y: 10,
            x0: n * 32,
            x1: (n + 1) * 32,
        })
        .collect();
    coalesce(&mut spans);
    assert_eq!(
        spans,
        vec![HoldingSpan {
            player: PlayerId(0),
            y: 10,
            x0: 0,
            x1: 224
        }]
    );
}

/// The row sweep before per-player active counts: the
/// output must stay identical.
fn reference_raster(
    sources: &[(PlayerId, TileCoord, i32)],
    width: i32,
    height: i32,
) -> BTreeMap<i32, Vec<HoldingSpan>> {
    let mut events = BTreeMap::<i32, Vec<(i32, usize, bool)>>::new();
    for (id, &(_, c, r)) in sources.iter().enumerate() {
        for y in (c.y - r).max(0)..=(c.y + r).min(height - 1) {
            let dy = y - c.y;
            let mut dx = r;
            while dx * dx + dy * dy > r * r {
                dx -= 1;
            }
            let (x0, x1) = ((c.x - dx).max(0), (c.x + dx + 1).min(width));
            if x0 < x1 {
                events
                    .entry(y)
                    .or_default()
                    .extend([(x0, id, true), (x1, id, false)]);
            }
        }
    }
    events
        .into_iter()
        .map(|(y, mut events)| {
            events.sort_unstable();
            let mut active = BTreeSet::<usize>::new();
            let mut row = Vec::new();
            let mut previous = 0;
            let key = |id: usize, x: i32| {
                let (player, c, _) = sources[id];
                let (dx, dy) = (i64::from(x - c.x), i64::from(y - c.y));
                (dx * dx + dy * dy, player, id)
            };
            for (x, id, add) in events {
                if previous < x
                    && let Some(&first) = active.first()
                {
                    if active.iter().all(|id| sources[*id].0 == sources[first].0) {
                        row.push(HoldingSpan {
                            player: sources[first].0,
                            y,
                            x0: previous,
                            x1: x,
                        });
                    } else {
                        let mut segments = vec![(previous, x, first)];
                        for &candidate in &active {
                            let mut next = Vec::new();
                            for (lo, hi, owner) in segments {
                                let wins = |at| key(candidate, at) < key(owner, at);
                                let left = wins(lo);
                                let right = wins(hi - 1);
                                if left == right {
                                    next.push((lo, hi, if left { candidate } else { owner }));
                                } else {
                                    let (mut a, mut b) = (lo + 1, hi - 1);
                                    while a < b {
                                        let mid = a + (b - a) / 2;
                                        if wins(mid) == left {
                                            a = mid + 1;
                                        } else {
                                            b = mid;
                                        }
                                    }
                                    next.extend([
                                        (lo, a, if left { candidate } else { owner }),
                                        (a, hi, if right { candidate } else { owner }),
                                    ]);
                                }
                            }
                            segments = next;
                        }
                        for (lo, hi, owner) in segments {
                            row.push(HoldingSpan {
                                player: sources[owner].0,
                                y,
                                x0: lo,
                                x1: hi,
                            });
                        }
                    }
                }
                if add {
                    active.insert(id);
                } else {
                    active.remove(&id);
                }
                previous = x;
            }
            coalesce(&mut row);
            (y, row)
        })
        .collect()
}

#[test]
fn the_row_sweep_matches_the_reference_on_dense_mixed_towns() {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..40 {
        let count = 1 + next(400) as usize;
        let sources: Vec<_> = (0..count)
            .map(|_| {
                (
                    PlayerId(next(6) as u16),
                    TileCoord::new(next(96) as i32 - 8, next(96) as i32 - 8),
                    next(24) as i32,
                )
            })
            .collect();
        assert_eq!(
            raster(&sources, 80, 80),
            reference_raster(&sources, 80, 80),
            "round {round}"
        );
    }
}

#[test]
fn an_epoch_re_rasters_only_changed_rows_and_matches_a_full_raster() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let source = |next: &mut dyn FnMut(u64) -> u64| {
        (
            PlayerId(next(6) as u16),
            TileCoord::new(next(96) as i32 - 8, next(96) as i32 - 8),
            next(24) as i32,
        )
    };
    let mut sources: Vec<_> = (0..300).map(|_| source(&mut next)).collect();
    let mut rows = raster(&sources, 80, 80);
    for round in 0..60 {
        let old = sources.clone();
        // Buildings finish, fall and change hands between two epochs.
        for _ in 0..1 + next(6) {
            match next(3) {
                0 => sources.push(source(&mut next)),
                1 if !sources.is_empty() => {
                    sources.remove(next(sources.len() as u64) as usize);
                }
                _ => {
                    let at = next(sources.len() as u64) as usize;
                    sources[at].0 = PlayerId(next(6) as u16);
                }
            }
        }
        raster_changed(&mut rows, &old, &sources, 80, 80);
        assert_eq!(rows, raster(&sources, 80, 80), "round {round}");
    }
}

fn land_world(side: i32) -> GameWorld {
    GameWorld::new(aoe_core::WorldConfig::new(side, side, aoe_core::Seed(7)).unwrap()).unwrap()
}

/// The buffer of each row of a viewer's kept drawing. A row drawn again while
/// the old drawing is still held cannot have the old row's buffer.
fn row_buffers(world: &GameWorld, viewer: PlayerId) -> BTreeMap<i32, usize> {
    let cache = world.economy.land.lock().unwrap();
    let (_, rows) = &cache.known[&viewer];
    rows.iter()
        .map(|(&y, row)| (y, row.as_ptr() as usize))
        .collect()
}

#[test]
fn a_viewers_report_redraws_only_the_rows_a_changed_source_covers() {
    let viewer = PlayerId(0);
    let mut world = land_world(256);
    let town = |world: &mut GameWorld, x: i32, y: i32| {
        let rect = aoe_core::TileRect::from_xywh(x, y, 4, 4);
        world
            .insert_building(viewer, BuildingKind::TownCenter, rect, true)
            .unwrap()
    };
    // Eight towns side by side: one more or less is at most an eighth of the
    // sources, short of the share that has everything drawn again (`kept`).
    for n in 0..8 {
        town(&mut world, 10 + 28 * n, 20);
    }
    world.land_control(viewer);
    let first = row_buffers(&world, viewer);
    assert_eq!(first.len(), 25, "a Town Center's disc covers 25 rows");
    // Sixty rows away: its disc shares no row with the others.
    let second = town(&mut world, 10, 80);
    world.tick = aoe_core::Tick(20);
    world.land_control(viewer);
    let both = row_buffers(&world, viewer);
    let redrawn = both
        .iter()
        .filter(|(y, at)| first.get(*y) != Some(*at))
        .count();
    assert_eq!((both.len(), redrawn), (50, 25), "(rows, rows redrawn)");
    // Its loss removes its rows and leaves the others where they are.
    world.remove_building(second);
    world.tick = aoe_core::Tick(40);
    world.land_control(viewer);
    assert_eq!(row_buffers(&world, viewer), first);
}

/// The report of a viewer nothing is kept for: every row drawn anew.
fn report_from_nothing(world: &GameWorld, viewer: PlayerId) -> Arc<LandControl> {
    let kept = std::mem::take(&mut *world.economy.land.lock().unwrap());
    let report = world.land_control(viewer);
    *world.economy.land.lock().unwrap() = kept;
    report
}

/// How a viewer's kept drawing follows its sources: in place, whole, shared.
mod kept;

#[test]
fn kept_influence_equals_a_full_drawing_through_building_loss_domain_player_and_fog_changes() {
    const SIDE: i32 = 160;
    const SEATS: u16 = 6;
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let mut world = land_world(SIDE);
    let explore = |world: &mut GameWorld, player: PlayerId, tile: TileCoord| {
        let at = aoe_core::WorldPosition::from_tile_center(tile).unwrap();
        if let Ok(scout) = world.spawn_unit(player, at) {
            world.advance();
            world.remove_unit(scout);
        }
    };
    // Five neighbours know each other's towns. The sixth seat only enters the
    // world with its first building.
    for seat in 0..SEATS - 1 {
        let rect = aoe_core::TileRect::from_xywh(30 + 12 * i32::from(seat), 60, 4, 4);
        world
            .insert_building(PlayerId(seat), BuildingKind::TownCenter, rect, true)
            .unwrap();
    }
    for seat in 0..SEATS - 1 {
        for town in 0..SEATS - 1 {
            let tile = TileCoord::new(36 + 12 * i32::from(town), 61);
            explore(&mut world, PlayerId(seat), tile);
        }
    }
    let kinds = [
        (BuildingKind::House, 2),
        (BuildingKind::Barracks, 3),
        (BuildingKind::WatchTower, 1),
        (BuildingKind::Outpost, 1),
        (BuildingKind::Castle, 4),
        (BuildingKind::TownCenter, 4),
    ];
    // Four scripted seconds reach every case; drawn changes follow them. Seat
    // 0's Steward sits in his hall, is let out (most of the width changes),
    // an Outpost is built (a tenth of it), and nothing happens.
    let hall = world.buildings().next().unwrap().id;
    let door = aoe_core::WorldPosition::from_tile_center(TileCoord::new(29, 61)).unwrap();
    let steward = crate::UnitKind::Steward;
    let steward = world.spawn_kind(PlayerId(0), steward, door).unwrap();
    world.issue_garrison(PlayerId(0), steward, hall).unwrap();
    let (mut foreign, mut kept, mut edited, mut whole, mut domains) = (0, 0, 0, 0, 0);
    for round in 0..44 {
        if round == 1 {
            world.issue_ungarrison(PlayerId(0), hall, None).unwrap();
        } else if round == 2 {
            let rect = aoe_core::TileRect::from_xywh(100, 120, 1, 1);
            let built = world.insert_building(PlayerId(0), BuildingKind::Outpost, rect, true);
            built.unwrap();
        }
        let changes = if round < 4 { 0 } else { 1 + next(4) };
        for _ in 0..changes {
            let player = PlayerId(next(u64::from(SEATS)) as u16);
            let standing = world
                .buildings()
                .map(|b| (b.id, b.kind, b.footprint))
                .collect::<Vec<_>>();
            match next(6) {
                0 | 1 => {
                    let (kind, side) = kinds[next(6) as usize];
                    let (x, y) = (8 + next(140) as i32, 8 + next(140) as i32);
                    let rect = aoe_core::TileRect::from_xywh(x, y, side, side);
                    let free = standing.iter().all(|(_, _, other)| {
                        rect.max.x <= other.min.x
                            || other.max.x <= rect.min.x
                            || rect.max.y <= other.min.y
                            || other.max.y <= rect.min.y
                    });
                    if free {
                        world.insert_building(player, kind, rect, true).unwrap();
                    }
                }
                2 if !standing.is_empty() => {
                    let (id, kind, rect) = standing[next(standing.len() as u64) as usize];
                    world.remove_building(id);
                    // Half of the losses change hands.
                    if next(2) == 0 {
                        world.insert_building(player, kind, rect, true).unwrap();
                    }
                }
                // A Steward in a Town Center holds a domain, until he is let
                // out. No technology or age moves a source: the only building
                // upgrades are the towers', of one radius.
                3 => {
                    let halls = world
                        .buildings()
                        .filter(|b| b.kind == BuildingKind::TownCenter)
                        .map(|b| (b.id, b.player, b.footprint.min, b.garrison.is_empty()))
                        .collect::<Vec<_>>();
                    if halls.is_empty() {
                        continue;
                    }
                    let (hall, owner, min, vacant) = halls[next(halls.len() as u64) as usize];
                    let door = TileCoord::new(min.x - 1, min.y + 1);
                    let door = aoe_core::WorldPosition::from_tile_center(door).unwrap();
                    if !vacant {
                        world.issue_ungarrison(owner, hall, None).unwrap();
                    } else if let Ok(steward) =
                        world.spawn_kind(owner, crate::UnitKind::Steward, door)
                    {
                        // Refused, he is taken away below.
                        let _ = world.issue_garrison(owner, steward, hall);
                        world.advance();
                        world.advance();
                    }
                }
                4 => {
                    world.set_fog_disabled(true);
                    world.land_control(player);
                    world.set_fog_disabled(false);
                }
                _ => {
                    let (x, y) = (4 + next(150) as i32, 4 + next(150) as i32);
                    explore(&mut world, player, TileCoord::new(x, y));
                }
            }
            // Whoever a door or a lost hall let out leaves the world.
            let outside = world.units().filter(|u| u.hp > 0).map(|u| u.id);
            for id in outside.collect::<Vec<_>>() {
                world.kill_unit(world.lookup[&id]);
            }
        }
        for _ in 0..20 {
            world.advance();
        }
        for viewer in (0..SEATS).map(PlayerId) {
            // A viewer not asked this second starts from older rows next time.
            if round >= 4 && next(4) == 0 {
                continue;
            }
            let fresh = report_from_nothing(&world, viewer);
            let before = world
                .economy
                .land
                .lock()
                .unwrap()
                .known
                .get(&viewer)
                .map(|known| known.0.clone());
            // The shared refresh comes first, so only this viewer's drawing
            // is counted below.
            world.held_land(viewer);
            let full = world.land_full_rasters();
            let report = world.land_control(viewer);
            let full = world.land_full_rasters() - full;
            assert_eq!(report, fresh, "round {round}, {viewer:?}");
            foreign += report.shapes.iter().filter(|s| s.player != viewer).count();
            let cache = world.economy.land.lock().unwrap();
            let (sources, rows) = &cache.known[&viewer];
            assert_eq!(
                **rows,
                reference_raster(sources, SIDE, SIDE),
                "round {round}, {viewer:?}"
            );
            domains += usize::from(
                sources
                    .iter()
                    .any(|source| source.2 == crate::STEWARD_RULES.radius_tiles),
            );
            match (before, full) {
                (Some(before), full) if before == *sources => {
                    assert_eq!(full, 0, "round {round}, {viewer:?}: same sources");
                    kept += 1;
                }
                (Some(_), 0) => edited += 1,
                (Some(_), 1) => whole += 1,
                (None, 1) => {}
                (_, full) => panic!("round {round}, {viewer:?}: {full} whole drawings"),
            }
        }
    }
    assert!(
        foreign > 0 && kept > 0 && edited > 0 && whole > 0 && domains > 0,
        "{foreign} foreign, {kept} kept, {edited} edited, {whole} whole, {domains} domains"
    );
}
