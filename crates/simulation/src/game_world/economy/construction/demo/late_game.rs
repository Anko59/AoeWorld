//! Late-game load for the real-match scale harness (`scale-match --late-game`).
//! A server-owned scenario hook, like `spawn_kind`: one seat gets an age, a
//! stockpile, finished buildings, walls, bridges and units around its village,
//! and its workers start on farms, wood, gold, stone and fish through the
//! ordinary gather orders. Placement walks fixed tile orders, so the same world
//! and seat give the same result.
use crate::{Age, BuildingKind, EconomyError, GameWorld, Stockpile, Tech, UnitKind};
use aoe_core::{BridgeSpan, PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_map::{ObjectKind, ResourceKind};

/// What one seat holds. Counts are targets: a tile or site that refuses a
/// unit or building is skipped, and `LateGameSeeded` says what was placed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LateGameSeat {
    pub age: Age,
    pub villagers: u32,
    pub fishing_ships: u32,
    pub army: Vec<(UnitKind, u32)>,
    /// Finished buildings besides walls and bridges (extra Town Centers too).
    pub buildings: Vec<(BuildingKind, u32)>,
    pub wall_tiles: u32,
    pub bridges: u32,
    /// Tiles from the village searched for building sites and unit tiles.
    pub radius: i32,
    /// Each resource the seat holds at the start.
    pub stockpile: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LateGameSeeded {
    pub units: u64,
    pub buildings: u64,
    pub wall_tiles: u64,
    pub bridges: u64,
    pub gather_orders: u64,
}

/// Bounds the bridge search: river samples tried per seat.
const BRIDGE_SAMPLES: usize = 4_096;
/// Ring (tiles from the village) the stone wall arcs stand on.
const WALL_RING: i32 = 34;
/// Least distance between two Town Centers of one seat, in tiles.
const TC_GAP: i32 = 30;

fn tile_center(tile: TileCoord) -> Result<WorldPosition, EconomyError> {
    WorldPosition::from_tile_center(tile).map_err(|_| EconomyError::InvalidFootprint)
}

/// Tile offsets ordered by distance, then y, then x (a fixed walk).
fn ring(from: i32, to: i32, step: usize) -> Vec<(i32, i32)> {
    let mut all = Vec::new();
    for dy in (-to..=to).step_by(step) {
        for dx in (-to..=to).step_by(step) {
            let d2 = dx * dx + dy * dy;
            if d2 >= from * from && d2 <= to * to {
                all.push((d2, dy, dx));
            }
        }
    }
    all.sort_unstable();
    all.into_iter().map(|(_, dy, dx)| (dx, dy)).collect()
}

impl GameWorld {
    /// Seeds `seat` around `center` for `player`. Off-tick setup only.
    pub fn seed_late_game(
        &mut self,
        player: PlayerId,
        center: TileCoord,
        seat: &LateGameSeat,
    ) -> Result<LateGameSeeded, EconomyError> {
        let mut seeded = LateGameSeeded::default();
        let ages: &[Tech] = match seat.age {
            Age::Dark => &[],
            Age::Feudal => &[Tech::FeudalAge],
            Age::Castle => &[Tech::FeudalAge, Tech::CastleAge],
            Age::Imperial => &[Tech::FeudalAge, Tech::CastleAge, Tech::ImperialAge],
        };
        for tech in ages {
            self.complete_research(player, *tech, &mut Vec::new());
        }
        if let Some(state) = self.economy.players.get_mut(&player) {
            let n = seat.stockpile;
            state.stockpile = Stockpile {
                food: n,
                wood: n,
                gold: n,
                stone: n,
            };
        }
        // Bridges first: their banks must still be open ground.
        seeded.bridges = self.seed_bridges(player, center, seat.radius, seat.bridges)?;
        let mut farms = Vec::new();
        let mut town_centers = vec![center];
        let sites = ring(6, seat.radius, 1);
        let mut next_site = 0;
        for &(kind, count) in &seat.buildings {
            let rules = kind.rules();
            let town_center = kind == BuildingKind::TownCenter;
            let mut placed = 0;
            let mut cursor = if town_center { 0 } else { next_site };
            while placed < count && cursor < sites.len() {
                let (dx, dy) = sites[cursor];
                cursor += 1;
                let origin = TileCoord::new(center.x + dx, center.y + dy);
                // Extra Town Centers spread out as an owner's expansions do.
                if town_center
                    && town_centers.iter().any(|t| {
                        (t.x - origin.x).pow(2) + (t.y - origin.y).pow(2) < TC_GAP * TC_GAP
                    })
                {
                    continue;
                }
                // A one-tile alley around each building keeps the base walkable.
                let alley = TileRect::new(
                    TileCoord::new(origin.x - 1, origin.y - 1),
                    TileCoord::new(origin.x + rules.side + 1, origin.y + rules.depth + 1),
                );
                if self.validate_footprint(alley).is_err() {
                    continue;
                }
                let rect = TileRect::new(
                    origin,
                    TileCoord::new(origin.x + rules.side, origin.y + rules.depth),
                );
                let id = self.insert_building(player, kind, rect, true)?;
                if kind == BuildingKind::Farm {
                    farms.push(id);
                }
                if town_center {
                    town_centers.push(origin);
                }
                placed += 1;
                seeded.buildings += 1;
            }
            if !town_center {
                next_site = cursor;
            }
        }
        self.refresh_population_cap(player);
        seeded.wall_tiles = self.seed_wall_arcs(player, center, seat.wall_tiles)?;
        let mut workers = Vec::new();
        let mut kinds = vec![(UnitKind::Villager, seat.villagers)];
        kinds.extend(seat.army.iter().copied());
        let mut kinds = kinds.into_iter().filter(|(_, n)| *n > 0).peekable();
        let mut left = kinds.peek().map_or(0, |(_, n)| *n);
        for (dx, dy) in ring(5, seat.radius, 1) {
            let Some(&(kind, _)) = kinds.peek() else {
                break;
            };
            // Workers stand close; soldiers every second tile, apart.
            if kind != UnitKind::Villager && (dx + dy) % 2 != 0 {
                continue;
            }
            let Ok(at) = tile_center(TileCoord::new(center.x + dx, center.y + dy)) else {
                continue;
            };
            if let Ok(id) = self.spawn_kind(player, kind, at) {
                seeded.units += 1;
                if kind == UnitKind::Villager {
                    workers.push(id);
                }
                left -= 1;
                if left == 0 {
                    kinds.next();
                    left = kinds.peek().map_or(0, |(_, n)| *n);
                }
            }
        }
        let mut ships = 0;
        for (dx, dy) in ring(4, seat.radius, 2) {
            if ships >= seat.fishing_ships {
                break;
            }
            let Ok(at) = tile_center(TileCoord::new(center.x + dx, center.y + dy)) else {
                continue;
            };
            if let Ok(id) = self.spawn_kind(player, UnitKind::FishingShip, at) {
                ships += 1;
                seeded.units += 1;
                let fish = self.nearest_resource_matching(
                    at.tile_floor(),
                    ResourceKind::Food,
                    64,
                    None,
                    |n| matches!(n.object, ObjectKind::ShoreFish | ObjectKind::DeepFish),
                );
                if let Some(node) = fish
                    && self.issue_gather(player, id, node.id).is_ok()
                {
                    seeded.gather_orders += 1;
                }
            }
        }
        seeded.gather_orders += self.seed_work(player, &workers, &farms);
        Ok(seeded)
    }

    /// One farmer per farm, the rest on wood, food, gold and stone in turn.
    fn seed_work(
        &mut self,
        player: PlayerId,
        workers: &[aoe_core::EntityId],
        farms: &[aoe_core::EntityId],
    ) -> u64 {
        const TURN: [ResourceKind; 6] = [
            ResourceKind::Wood,
            ResourceKind::Wood,
            ResourceKind::Food,
            ResourceKind::Gold,
            ResourceKind::Wood,
            ResourceKind::Stone,
        ];
        let mut orders = 0;
        let (farmers, rest) = workers.split_at(farms.len().min(workers.len()));
        for (worker, farm) in farmers.iter().zip(farms) {
            orders += u64::from(self.issue_farm(player, *worker, *farm).is_ok());
        }
        for (n, worker) in rest.iter().enumerate() {
            let Some(at) = self.unit(*worker).map(|u| u.position.tile_floor()) else {
                continue;
            };
            let kind = TURN[n % TURN.len()];
            if let Some(node) = self.nearest_resource(at, kind, 48, None) {
                orders += u64::from(self.issue_gather(player, *worker, node.id).is_ok());
            }
        }
        orders
    }

    /// Stone wall arcs on a ring around the village, with a gap every eight
    /// tiles so the village stays open.
    fn seed_wall_arcs(
        &mut self,
        player: PlayerId,
        center: TileCoord,
        tiles: u32,
    ) -> Result<u64, EconomyError> {
        let mut placed = 0;
        let mut ring_tiles: Vec<_> = ring(WALL_RING, WALL_RING + 1, 1)
            .into_iter()
            .map(|(dx, dy)| ((f64::from(dy)).atan2(f64::from(dx)), dx, dy))
            .collect();
        ring_tiles.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));
        for (n, (_, dx, dy)) in ring_tiles.into_iter().enumerate() {
            if placed >= u64::from(tiles) {
                break;
            }
            if n % 8 == 7 {
                continue;
            }
            let rect = TileRect::from_xywh(center.x + dx, center.y + dy, 1, 1);
            if self.validate_footprint(rect).is_ok() {
                self.insert_building(player, BuildingKind::StoneWall, rect, true)?;
                placed += 1;
            }
        }
        Ok(placed)
    }

    /// Finished cardinal bridges over river water within `radius` tiles.
    fn seed_bridges(
        &mut self,
        player: PlayerId,
        center: TileCoord,
        radius: i32,
        count: u32,
    ) -> Result<u64, EconomyError> {
        let mut placed = 0;
        let mut used: Vec<BridgeSpan> = Vec::new();
        for (dx, dy) in ring(8, radius, 4).into_iter().take(BRIDGE_SAMPLES) {
            if placed >= u64::from(count) {
                break;
            }
            let at = TileCoord::new(center.x + dx, center.y + dy);
            if self.demo_water(at) != Some(true) {
                continue;
            }
            for (ax, ay) in [(1, 0), (0, 1)] {
                let bank = |sign: i32| {
                    (1..=20)
                        .map(|n| TileCoord::new(at.x + sign * ax * n, at.y + sign * ay * n))
                        .find(|&t| self.demo_water(t) != Some(true))
                        .filter(|&t| self.demo_water(t) == Some(false))
                };
                let (Some(from), Some(to)) = (bank(-1), bank(1)) else {
                    continue;
                };
                let Some(span) = BridgeSpan::new(from, to) else {
                    continue;
                };
                let footprint = span.footprint();
                let near = used.iter().any(|s| {
                    let f = s.footprint();
                    (f.min.x - footprint.min.x).abs() + (f.min.y - footprint.min.y).abs() < 24
                });
                if near || self.place_bridge_for(player, span).is_err() {
                    continue;
                }
                used.push(span);
                placed += 1;
                break;
            }
        }
        Ok(placed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat() -> LateGameSeat {
        LateGameSeat {
            age: Age::Castle,
            villagers: 30,
            fishing_ships: 2,
            army: vec![(UnitKind::Pikeman, 20), (UnitKind::Crossbowman, 20)],
            buildings: vec![
                (BuildingKind::TownCenter, 2),
                (BuildingKind::Mill, 2),
                (BuildingKind::Farm, 6),
                (BuildingKind::House, 10),
            ],
            wall_tiles: 40,
            bridges: 1,
            radius: 70,
            stockpile: 1_000,
        }
    }

    fn seeded() -> (GameWorld, LateGameSeeded) {
        let (mut world, scout) = GameWorld::default_with_village(aoe_core::Seed(7));
        let center = world.unit(scout).map(|u| u.position.tile_floor()).unwrap();
        let seeded = world.seed_late_game(PlayerId(0), center, &seat()).unwrap();
        (world, seeded)
    }

    #[test]
    fn late_game_seeding_is_deterministic_and_places_the_shape() {
        let (a, first) = seeded();
        let (b, second) = seeded();
        assert_eq!(first, second);
        assert_eq!(a.canonical_hash(), b.canonical_hash());
        assert!(first.units >= 70, "{first:?}");
        assert!(first.buildings >= 15, "{first:?}");
        assert!(first.wall_tiles >= 30, "{first:?}");
        // The default world has no resource nodes: only the farmers start.
        assert_eq!(first.gather_orders, 6, "{first:?}");
        assert_eq!(
            a.player_economy(PlayerId(0)).map(|p| p.age),
            Some(Age::Castle)
        );
    }
}
