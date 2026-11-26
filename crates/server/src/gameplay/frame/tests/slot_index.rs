//! The paged index from unit ids to slots, on its own and under a population.
use super::*;

const END: u32 = INDEX_PAGE - 1;

/// `(ids with a slot, pages held)`.
fn held_by(index: &SlotIndex) -> (usize, usize) {
    (index.len(), index.pages())
}

fn assert_absent(index: &SlotIndex, raw: u32) {
    assert_eq!(index.get(EntityId(raw)), None, "id {raw}");
    assert_eq!(index[&EntityId(raw)], ABSENT, "id {raw}");
}

#[test]
fn ids_stay_apart_at_page_ends_and_absent_ones_are_answered() {
    let mut index = SlotIndex::default();
    // Nothing is held: any id is absent, and removing one changes nothing.
    for raw in [0, 1, END, INDEX_PAGE, 1_000_000, u32::MAX] {
        assert_absent(&index, raw);
        index.remove(EntityId(raw));
    }
    assert_eq!(held_by(&index), (0, 0));
    // Id 0, both sides of a page's end, and a far id over empty pages.
    let far = 4_000 * INDEX_PAGE + END;
    let ids = [0, END, INDEX_PAGE, 2 * INDEX_PAGE - 1, far];
    for (slot, raw) in (40..).zip(ids) {
        index.insert(EntityId(raw), slot);
    }
    assert_eq!(held_by(&index), (5, 3));
    for (slot, raw) in (40..).zip(ids) {
        assert_eq!(index.get(EntityId(raw)), Some(slot), "id {raw}");
        assert_eq!(index[&EntityId(raw)], slot, "id {raw}");
    }
    for raw in [1, END - 1, INDEX_PAGE + 1, 2 * INDEX_PAGE, far - 1, far + 1] {
        assert_absent(&index, raw);
    }
    // Another slot for an id it has replaces the first one.
    index.insert(EntityId(END), 7);
    assert_eq!(index.get(EntityId(END)), Some(7));
    assert_eq!(held_by(&index), (5, 3));
    // Removing an id twice takes nothing from the one it shares a page with.
    for _ in 0..2 {
        index.remove(EntityId(0));
        assert_absent(&index, 0);
        assert_eq!(index.get(EntityId(END)), Some(7));
        assert_eq!(held_by(&index), (4, 3));
    }
    // The last id of a page frees it, and only once.
    for _ in 0..2 {
        index.remove(EntityId(END));
        assert_absent(&index, END);
        assert_eq!(held_by(&index), (3, 2));
    }
    // A page held again remembers nothing of before.
    index.insert(EntityId(3), 9);
    assert_eq!(index.get(EntityId(3)), Some(9));
    assert_absent(&index, 0);
    assert_absent(&index, END);
    assert_eq!(held_by(&index), (4, 3));
}

#[test]
fn a_dense_run_fills_its_pages_and_frees_them_one_by_one() {
    // Three pages and one id of a fourth.
    let mut index = SlotIndex::default();
    let run = 3 * INDEX_PAGE + 1;
    for raw in 0..run {
        index.insert(EntityId(raw), run - raw);
    }
    assert_eq!(held_by(&index), (run as usize, 4));
    for raw in 0..run {
        assert_eq!(index.get(EntityId(raw)), Some(run - raw), "id {raw}");
    }
    assert_absent(&index, run);
    for raw in 0..run {
        index.remove(EntityId(raw));
        let left = run - raw - 1;
        // The page of the next id and those after it are still held.
        let pages = if left == 0 {
            0
        } else {
            4 - (raw + 1) / INDEX_PAGE
        };
        let expected = (left as usize, pages as usize);
        assert_eq!(held_by(&index), expected, "after id {raw}");
    }
}

#[test]
fn pages_emptied_and_filled_again_match_an_ordered_map() {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n) as u32
    };
    // Few ids in few pages, so pages empty over and over.
    let places = [0, 1, END];
    let mut index = SlotIndex::default();
    let mut model = BTreeMap::<u32, u32>::new();
    let mut emptied = 0;
    for step in 0..4_000 {
        let raw = next(5) * INDEX_PAGE + places[next(3) as usize];
        let before = index.pages();
        if next(5) < 2 {
            index.insert(EntityId(raw), step);
            model.insert(raw, step);
        } else {
            index.remove(EntityId(raw));
            model.remove(&raw);
        }
        for page in 0..5 {
            for at in places {
                let id = page * INDEX_PAGE + at;
                let expected = model.get(&id).copied();
                assert_eq!(index.get(EntityId(id)), expected, "step {step}");
            }
        }
        let pages = model.keys().map(|id| id / INDEX_PAGE);
        let pages = pages.collect::<BTreeSet<_>>().len();
        assert_eq!(held_by(&index), (model.len(), pages), "step {step}");
        emptied += usize::from(index.pages() < before);
    }
    assert!(emptied > 100, "{emptied} pages emptied");
}

#[test]
fn a_dead_populations_pages_leave_the_frame() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    world.set_fog_disabled(true);
    let spawn = |world: &mut GameWorld, n: i32| {
        let tile = aoe_core::TileCoord::new(10 + n % 48 * 4, 10 + n / 48 * 4);
        let at = aoe_core::WorldPosition::from_tile_center(tile).unwrap();
        let id = world.spawn_unit(PlayerId(0), at).unwrap();
        world
            .issue_stance(PlayerId(0), &[id], aoe_core::CombatStance::NoAttack)
            .unwrap();
        id
    };
    // Three full pages of ids.
    let mut ids = Vec::new();
    for n in 0..3 * INDEX_PAGE as i32 {
        ids.push(spawn(&mut world, n));
    }
    assert_eq!((ids[0].0, ids[767].0), (0, 767), "ids start at 0");
    world.advance();
    let mut frame = UnitFrame::build(&world, 1, 1, None).unwrap();
    assert_eq!(held_by(&frame.index), (768, 3));
    // The whole middle page dies, and all of the first but its last id.
    let mut dead = ids[256..512].to_vec();
    dead.extend(&ids[..255]);
    world.issue_delete(PlayerId(0), &dead).unwrap();
    let ticks = u64::from(world.config().tick_hz) * (u64::from(aoe_simulation::CORPSE_SECONDS) + 4);
    // Every tick builds on the frame before: one built from nothing would have
    // a new index, whatever the old one kept.
    for generation in 2..=ticks {
        world.advance();
        frame = UnitFrame::build(&world, 1, generation, Some(frame)).unwrap();
        assert_eq!(frame.visited(), world.unit_changes().len());
        frame.assert_matches_world(&world);
    }
    assert_eq!(world.unit_count(), 257, "the corpses are gone");
    assert_eq!(held_by(&frame.index), (257, 2));
    assert!(dead.iter().all(|id| frame.index.get(*id).is_none()));
    // Their slots serve later ids, which sit in a page of their own.
    let late = spawn(&mut world, 768);
    assert_eq!(late.0, 3 * INDEX_PAGE);
    world.advance();
    frame = UnitFrame::build(&world, 1, ticks + 1, Some(frame)).unwrap();
    frame.assert_matches_world(&world);
    assert_eq!(held_by(&frame.index), (258, 3));
    assert!(frame.index.get(late).is_some_and(|slot| slot < 768));
}
