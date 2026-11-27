//! The worlds of the fog cycle tests, the one-by-one reference, and the
//! comparison of everything a cycle leaves behind.
use crate::game_world::snapshot::AnimalRecord;
use crate::{AnimalKind, BuildingKind, Facing, GameWorld, Terrain, UnitKind};
use aoe_core::{
    BridgeSpan, EntityId, PlayerId, Seed, TileCoord, TileRect, WorldConfig, WorldPosition,
};
use aoe_map::{Depletion, MapChunkGenerator, ResourceOverlay};
use std::collections::{BTreeMap, BTreeSet};

/// Explored a little in `scene` and sees nothing any more.
pub(super) const BLIND: PlayerId = PlayerId(7);
const WORLD: TileRect = TileRect::from_xywh(0, 0, 256, 256);
const VIEWS: [TileRect; 3] = [
    TileRect::from_xywh(96, 96, 40, 40),
    TileRect::from_xywh(24, 16, 16, 40),
    TileRect::from_xywh(200, 200, 40, 40),
];

pub(super) fn world() -> GameWorld {
    GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap()
}

pub(super) fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

pub(super) fn house(w: &mut GameWorld, player: u16, x: i32, y: i32, complete: bool) -> EntityId {
    let footprint = TileRect::from_xywh(x, y, 2, 2);
    w.insert_building(PlayerId(player), BuildingKind::House, footprint, complete)
        .unwrap()
}

pub(super) fn sheep(w: &mut GameWorld, x: i32, y: i32, owner: Option<u16>) {
    let animal = AnimalRecord {
        id: EntityId(w.next_entity_id as u32),
        kind: AnimalKind::Sheep,
        owner: owner.map(PlayerId),
        hp: 7,
        position: at(x, y),
        food_remaining: 100,
        facing: Facing::South,
        home: at(x, y),
    };
    assert!(w.restore_animal(&animal));
}

pub(super) fn watchers(w: &GameWorld) -> BTreeMap<EntityId, BTreeSet<PlayerId>> {
    w.vision
        .building_watchers
        .iter()
        .map(|(id, watched)| (*id, watched.watchers.clone()))
        .collect()
}

/// Units, buildings and animals that observe now.
pub(super) fn observers(w: &GameWorld) -> usize {
    (0..w.next_entity_id)
        .filter(|id| w.vision.observers.get(&EntityId(*id as u32)).is_some())
        .count()
}

/// The militia and the monk of `scene`, each alone and far from everyone.
pub(super) fn sleepers(w: &GameWorld) -> [EntityId; 2] {
    [UnitKind::Militia, UnitKind::Monk].map(|kind| {
        let mut found = w.units().filter(|u| u.kind == kind);
        let id = found.next().unwrap().id;
        assert!(found.next().is_none(), "one {kind:?}");
        id
    })
}

/// The healers that look for wounds on the next healing pass; every other
/// healer rests. Asking changes nothing while sleep is on.
pub(super) fn healers_awake(w: &mut GameWorld) -> Vec<EntityId> {
    w.combat.healing.ready(false, &BTreeSet::new())
}

/// With fog on: five players' houses and scouts among each other, foundations
/// in every state, two angled bridges, herds, a player who sees nothing, and
/// two houses player 0 remembers unseen; the first of them is a ghost already.
pub(super) fn scene() -> (GameWorld, [EntityId; 2]) {
    let mut w = world();
    // Far from everyone: a healer that rests until its sight is new. It looks
    // once, on a healing pass, before anyone else is there.
    let monk = w
        .spawn_kind(PlayerId(0), UnitKind::Monk, at(220, 100))
        .unwrap();
    for _ in 0..16 {
        w.advance();
    }
    assert!(!healers_awake(&mut w).contains(&monk), "a lone monk rests");
    let remembered = [100, 106].map(|y| house(&mut w, 1, 30, y, true));
    let scout = w.spawn_unit(PlayerId(0), at(33, 103)).unwrap();
    w.advance();
    w.remove_unit(scout);
    w.advance();
    w.remove_building(remembered[0]);
    // A scout sees its own corner house and the two next to it.
    for n in 0..20_u16 {
        let (i, j) = (i32::from(n % 5), i32::from(n / 5));
        house(&mut w, n % 5, 100 + 6 * i, 100 + 6 * j, true);
        let scout = PlayerId((n + n / 5 + 1) % 5);
        w.spawn_unit(scout, at(103 + 6 * i, 103 + 6 * j)).unwrap();
    }
    // Planned (private, in an enemy's sight), started and half built.
    house(&mut w, 2, 103, 100, false);
    let sites = [(3, 109), (4, 115)].map(|(player, x)| house(&mut w, player, x, 100, false));
    for site in sites {
        w.start_foundation(site).unwrap();
    }
    let half = w.building(sites[1]).unwrap().work_total / 2;
    w.building_mut(sites[1]).unwrap().work_done = half;
    w.update_building_vision(sites[1]);
    // Player 1 sees the deck, player 0 only a corner of the rectangle, where
    // player 4's wall stands beside the bridge.
    for (y, complete) in [(20, true), (40, false)] {
        let span = BridgeSpan::new(TileCoord::new(29, y), TileCoord::new(34, y + 3)).unwrap();
        let bridge = w
            .insert_building_shape(
                PlayerId(3),
                BuildingKind::Bridge,
                span.footprint(),
                complete,
                Some(span),
            )
            .unwrap();
        w.start_foundation(bridge).unwrap();
        let wall = TileRect::from_xywh(29, y + 3, 1, 1);
        w.insert_building(PlayerId(4), BuildingKind::PalisadeWall, wall, true)
            .unwrap();
        w.spawn_unit(PlayerId(0), at(29, y + 5)).unwrap();
        w.spawn_unit(PlayerId(1), at(31, y - 2)).unwrap();
    }
    for (x, owner) in [(60, Some(0)), (64, Some(3)), (68, None)] {
        sheep(&mut w, x, 60, owner);
    }
    // Far from everyone: a fighter that sleeps until its sight is new.
    w.spawn_militia(PlayerId(0), at(220, 220)).unwrap();
    let lost = w.spawn_unit(BLIND, at(103, 97)).unwrap();
    w.remove_unit(lost);
    w.remember_visible_buildings();
    (w, remembered)
}

/// The scene with fog off, changed where no player's sight followed: what
/// turning fog on again has to find.
pub(super) fn hidden() -> GameWorld {
    let (mut w, remembered) = scene();
    w.set_fog_disabled(true);
    w.remove_building(remembered[1]);
    // Revisits the first ghost's empty site and not the second's.
    w.spawn_unit(PlayerId(0), at(33, 98)).unwrap();
    // A player without any sight storage yet.
    w.spawn_unit(PlayerId(6), at(103, 94)).unwrap();
    let dead = w.spawn_unit(PlayerId(2), at(80, 80)).unwrap();
    w.kill_unit(w.lookup[&dead]);
    let hit = w.buildings().find(|b| b.is_complete()).unwrap().id;
    w.building_mut(hit).unwrap().hp -= 100;
    let _ = w.take_remembered_building_changes();
    // A changed resource node in player 0's coming sight.
    let tile = TileCoord::new(33, 100);
    let mut overlay = ResourceOverlay::default();
    overlay.apply_depletion(&Depletion {
        id: (tile.y as u64) << 19 | (tile.x as u64) << 1,
        removed: 1,
        added: 0,
        remaining: 99,
        became_nonblocking: false,
        revision: 1,
    });
    w.terrain = Terrain::Map {
        generator: MapChunkGenerator::new([3; 32], 1, 256),
        overlay,
    };
    // Both sleep until fog returns: waking is what the cycle has to do.
    let [militia, monk] = sleepers(&w);
    assert!(w.combat.quiet.resting(militia), "the militia sleeps");
    assert!(!healers_awake(&mut w).contains(&monk), "the monk rests");
    w
}

/// `hidden` with fog on again as before the vision batch: every disc applied
/// alone, units, then buildings, then living owned animals.
pub(super) fn one_by_one() -> GameWorld {
    let mut w = hidden();
    w.vision.disabled = false;
    w.vision.revision += 1;
    w.vision.observers.clear();
    for vision in w.vision.players.values_mut() {
        vision.revision += 1;
        vision.reset_stamp = vision.revision;
        vision.visible.clear();
        vision.visible_tiles = 0;
        vision.overview.clear_counts();
    }
    w.rebuild_building_sight_index();
    for watched in w.vision.building_watchers.values() {
        assert_eq!(watched.watchers, BTreeSet::from([watched.owner]));
    }
    let units = w.units().map(|u| u.id).collect::<Vec<_>>();
    let buildings = w.buildings().map(|b| b.id).collect::<Vec<_>>();
    for id in units {
        w.update_unit_vision(id);
    }
    for id in buildings {
        w.update_building_vision(id);
    }
    let animals = w
        .animals()
        .filter(|a| a.hp > 0)
        .filter_map(|a| {
            a.owner
                .map(|p| (a.id, p, a.position.tile_floor(), a.kind.rules().los_tiles))
        })
        .collect::<Vec<_>>();
    for (id, player, tile, los) in animals {
        w.update_observer(id, Some((player, tile, los)));
    }
    w.remember_visible_buildings();
    w
}

/// Everything a fog cycle leaves behind, public answers and derived state.
pub(super) fn assert_same(label: &str, got: &mut GameWorld, want: &mut GameWorld) {
    let same = |what: &str, equal: bool| assert!(equal, "{label}: {what}");
    same("healers", healers_awake(got) == healers_awake(want));
    let (a, b) = (&*got, &*want);
    let (ours, theirs) = (&a.vision, &b.vision);
    same("hash", a.canonical_hash() == b.canonical_hash());
    same("revision", a.vision_revision() == b.vision_revision());
    same("open queue", ours.batch.is_none());
    same("watchers", watchers(a) == watchers(b));
    // Empty on both sides: a late disc would leave its new watch here.
    same("dirty", ours.dirty_buildings == theirs.dirty_buildings);
    same("index", ours.building_at_tile == theirs.building_at_tile);
    let (plans, reference) = (&ours.private_foundations, &theirs.private_foundations);
    same("private sites", plans == reference);
    for id in 0..b.next_entity_id {
        let id = EntityId(id as u32);
        let (seen, known) = (a.vision.observers.get(&id), b.vision.observers.get(&id));
        assert_eq!(seen, known, "{label}: observer {id:?}");
        let (rests, rested) = (a.combat.quiet.resting(id), b.combat.quiet.resting(id));
        assert_eq!(rests, rested, "{label}: sleep of {id:?}");
    }
    same("players", ours.players.keys().eq(theirs.players.keys()));
    for &player in theirs.players.keys() {
        assert_same_player(label, a, b, player);
    }
    let changes = got.take_remembered_building_changes();
    let reference = want.take_remembered_building_changes();
    assert_eq!(changes, reference, "{label}: memory changes");
}

fn assert_same_player(label: &str, a: &GameWorld, b: &GameWorld, player: PlayerId) {
    let same = |what: &str, equal: bool| assert!(equal, "{label}, {player:?}: {what}");
    let tiles = |w: &GameWorld| w.visibility_tiles(player, WORLD);
    same("tiles", tiles(a) == tiles(b));
    let overview = |w: &GameWorld| w.vision_overview(player);
    same("overview", overview(a) == overview(b));
    let revision = |w: &GameWorld| w.player_vision_revision(player);
    same("revision", revision(a) == revision(b));
    let storage = |w: &GameWorld| w.vision_storage(player);
    same("storage", storage(a) == storage(b));
    let known = |w: &GameWorld| w.known_buildings(player);
    same("known buildings", known(a) == known(b));
    let nodes = |w: &GameWorld| w.known_resource_amounts(player);
    same("known resources", nodes(a) == nodes(b));

    let (ours, theirs) = (&a.vision.players[&player], &b.vision.players[&player]);
    same("stamps", ours.fog_stamps == theirs.fog_stamps);
    same("pixels", ours.overview_stamp == theirs.overview_stamp);
    same("reset stamp", ours.reset_stamp == theirs.reset_stamp);
    same("memory", ours.remembered == theirs.remembered);
    same("changed", ours.changed == theirs.changed);
    same("ghost tiles", ours.ghost_tiles == theirs.ghost_tiles);
    same("ghost sites", ours.ghost_sites == theirs.ghost_sites);
    same("ghost order", ours.ghost_order == theirs.ghost_order);
    same("chunks", ours.visible.len() == theirs.visible.len());
    for (chunk, counters) in &theirs.visible {
        let mine = ours.visible.get(chunk).map(|mine| &mine.counts);
        same("counters", mine == Some(&counters.counts));
    }
    for since in 0..=theirs.revision {
        for view in VIEWS {
            let fog = |w: &GameWorld| w.fog_changed_since(player, view, since);
            same("fog view", fog(a) == fog(b));
        }
    }
}
