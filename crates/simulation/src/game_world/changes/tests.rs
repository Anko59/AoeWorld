use super::*;
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};

fn world() -> GameWorld {
    GameWorld::new(WorldConfig::new(128, 128, Seed(7)).unwrap()).unwrap()
}
fn at(x: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, 10)).unwrap()
}

#[test]
fn journal_retains_add_remove_and_displaced_storage_slot_then_clears_idle_ticks() {
    let mut world = world();
    let first = world.spawn_unit(PlayerId(0), at(10)).unwrap();
    let last = world.spawn_unit(PlayerId(0), at(12)).unwrap();
    world.advance();
    assert_eq!(world.unit_changes(), &[first, last]);
    world.advance();
    assert!(world.unit_changes().is_empty());
    world.remove_unit(first);
    world.advance();
    assert_eq!(world.unit_changes(), &[first, last]);
    assert_eq!(world.unit_storage_slot(last), Some(0));
    world.advance();
    assert!(world.unit_changes().is_empty());
}

#[test]
fn report_only_changes_and_member_dependencies_are_published_without_unit_writes() {
    let mut world = world();
    let member = world.spawn_unit(PlayerId(0), at(10)).unwrap();
    let general = world.spawn_unit(PlayerId(0), at(12)).unwrap();
    world.advance();
    world.advance();
    world.economy.generals.controlled.insert(member, general);
    world.advance();
    assert_eq!(world.unit_changes(), &[member, general]);
    world.advance();
    assert!(world.unit_changes().is_empty());
    let index = world.lookup[&member];
    world.units[index].state.hp -= 1;
    world.advance();
    assert_eq!(world.unit_changes(), &[member, general]);
    world.economy.generals.controlled.remove(&member);
    world.advance();
    assert!(world.unit_changes().contains(&member));
}

// Determinism tests deliberately permute storage.
impl Units {
    pub(crate) fn swap(&mut self, a: usize, b: usize) {
        self.dirty.insert(self.values[a].state.id);
        self.dirty.insert(self.values[b].state.id);
        self.values.swap(a, b);
    }
}

#[test]
fn borrowed_mutable_views_journal_every_exposed_unit_and_reads_stay_clean() {
    let mut world = world();
    let ids = (10..14)
        .map(|x| world.spawn_unit(PlayerId(0), at(x)).unwrap())
        .collect::<Vec<_>>();
    world.advance();
    world.advance();
    for unit in &world.units {
        std::hint::black_box(unit);
    }
    std::hint::black_box(&world.units[..2]);
    world.advance();
    assert!(world.unit_changes().is_empty());

    std::hint::black_box(&mut world.units[2..]);
    world.advance();
    assert_eq!(world.unit_changes(), &ids[2..]);
    std::hint::black_box(world.units.as_mut_slice());
    world.advance();
    assert_eq!(world.unit_changes(), ids);
    for unit in &mut world.units {
        std::hint::black_box(unit);
    }
    world.advance();
    assert_eq!(world.unit_changes(), ids);
    world.advance();
    assert!(world.unit_changes().is_empty());
}

// Keep bulk mutable access test-only: production writes use the journal's
// explicit methods and cannot escape through an unjournaled DerefMut.
impl Units {
    pub(crate) fn reset_accesses(&self) {
        self.values.reset_accesses();
    }
    pub(crate) fn accesses(&self) -> usize {
        self.values.accesses()
    }
    pub(crate) fn oracle_slice(&self) -> &[StoredUnit] {
        self.values.oracle_slice()
    }
    pub(crate) fn as_slice(&self) -> &[StoredUnit] {
        &self.values
    }
    pub(crate) fn as_mut_slice(&mut self) -> &mut [StoredUnit] {
        self.mark_all();
        self.values.as_mut_slice()
    }
}
impl IndexMut<std::ops::RangeFrom<usize>> for Units {
    fn index_mut(&mut self, range: std::ops::RangeFrom<usize>) -> &mut Self::Output {
        let units = &mut self.values[range];
        self.dirty.extend(units.iter().map(|unit| unit.state.id));
        units
    }
}
