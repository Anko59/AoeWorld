use super::super::counters::Income;
use super::{BoxError, gold};
use aoe_core::PlayerId;
use aoe_map::ResourceNode;
use aoe_simulation::{Age, BuildingKind, GameUnit, GameWorld, Stockpile, TECH_RULES, UnitKind};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct Sample {
    pub tick: u64,
    pub players: Vec<PlayerSample>,
}

#[derive(Serialize)]
pub(super) struct PlayerSample {
    pub player: PlayerId,
    pub age: Age,
    pub age_ticks: [Option<u64>; 4],
    pub stockpile: Stockpile,
    pub home_gold: gold::Gold,
    pub delivered: [u64; 4],
    pub town_centers: u32,
    pub villagers: u32,
    pub army: u32,
    pub food_upkeep_per_minute: u32,
    pub food_upkeep_shortage: bool,
    pub research: Vec<Option<u64>>,
    pub first_contact: Option<Contact>,
    pub first_five_tcs: Option<u64>,
    pub first_six_tcs: Option<u64>,
}

#[derive(Clone, Serialize)]
pub(super) struct Contact {
    pub tick: u64,
    pub opponent: PlayerId,
    pub town_centers: u32,
    pub technologies: u32,
    pub age: Age,
    pub army: u32,
    pub villagers: u32,
}

struct Seat {
    income: Income,
    delivered: [u64; 4],
    age_ticks: [Option<u64>; 4],
    researched: u128,
    research: Vec<Option<u64>>,
    first_contact: Option<Contact>,
    first_five_tcs: Option<u64>,
    first_six_tcs: Option<u64>,
}

pub(super) struct Recorder {
    seats: Vec<Seat>,
    gold_sites: Vec<Vec<ResourceNode>>,
    next: u64,
}

#[derive(Clone, Copy, Default)]
struct Census {
    town_centers: u32,
    villagers: u32,
    army: u32,
}

impl Census {
    fn count(&mut self, unit: GameUnit) {
        if unit.hp == 0 {
            return;
        }
        if unit.kind == UnitKind::Villager {
            self.villagers += 1;
        } else if unit.kind.needs_food_upkeep() {
            self.army += 1;
        }
    }
}

impl Recorder {
    pub(super) fn new(world: &GameWorld, seats: usize) -> Result<Self, BoxError> {
        if world.tick().0 != 0
            || world.players().count() != seats
            || (0..seats).any(|id| world.player_economy(PlayerId(id as u16)).is_none())
        {
            return Err("balance-match needs a fresh village for every seat".into());
        }
        let gold_sites = gold::survey(world, seats)?;
        let seats = (0..seats)
            .map(|_| Seat {
                income: Income::new(world.config().tick_hz),
                delivered: [0; 4],
                age_ticks: [None; 4],
                researched: 0,
                research: vec![None; TECH_RULES.len()],
                first_contact: None,
                first_five_tcs: None,
                first_six_tcs: None,
            })
            .collect();
        Ok(Self {
            seats,
            gold_sites,
            next: 0,
        })
    }

    pub(super) fn observe(
        &mut self,
        world: &GameWorld,
        scheduled: bool,
    ) -> Result<Option<Sample>, BoxError> {
        let tick = world.tick().0;
        if tick != self.next {
            return Err(format!("balance-match: expected tick {}, got {tick}", self.next).into());
        }
        self.next += 1;
        let mut emit = scheduled;
        for player in world.players() {
            let seat = &mut self.seats[usize::from(player.player.0)];
            seat.age_ticks[player.age as usize].get_or_insert(tick);
            let (credits, fault) = seat
                .income
                .credits(tick, world.recent_deliveries(player.player));
            if fault {
                return Err(format!(
                    "balance-match: inconsistent delivery history at tick {tick}, seat {}",
                    player.player.0
                )
                .into());
            }
            for (total, amount) in seat.delivered.iter_mut().zip(credits) {
                *total += amount;
            }
            if player.researched != seat.researched {
                for (rules, completed) in TECH_RULES.iter().zip(&mut seat.research) {
                    if player.researched & rules.tech.bit() != 0 {
                        completed.get_or_insert(tick);
                    }
                }
                seat.researched = player.researched;
            }
        }

        let mut towns = vec![0; self.seats.len()];
        let mut opponents = vec![None; self.seats.len()];
        let awaiting: Vec<_> = self
            .seats
            .iter()
            .enumerate()
            .filter_map(|(i, seat)| seat.first_contact.is_none().then_some(i))
            .collect();
        let buildings: Vec<_> = world.buildings().collect();
        for building in &buildings {
            let owner = usize::from(building.player.0);
            if owner >= self.seats.len() {
                continue;
            }
            if building.kind == BuildingKind::TownCenter && building.is_complete() {
                towns[owner] += 1;
            }
            if building.hp > 0 && !building.is_unstarted() {
                for &viewer in &awaiting {
                    if viewer != owner
                        && opponents[viewer].is_none()
                        && world.building_visible_to(PlayerId(viewer as u16), building.footprint)
                    {
                        opponents[viewer] = Some(building.player);
                    }
                }
            }
        }
        if awaiting.iter().any(|&viewer| opponents[viewer].is_none()) {
            for unit in world.units().filter(|unit| unit.hp > 0) {
                let owner = usize::from(unit.player.0);
                if owner >= self.seats.len() {
                    continue;
                }
                for &viewer in &awaiting {
                    if viewer != owner
                        && opponents[viewer].is_none()
                        && world.visibility(PlayerId(viewer as u16), unit.position.tile_floor())
                            == aoe_simulation::TileVisibility::Visible
                        && world.unit_visible_to(PlayerId(viewer as u16), unit.id)
                    {
                        opponents[viewer] = Some(unit.player);
                    }
                }
            }
        }
        for (seat, towns) in self.seats.iter_mut().zip(&towns) {
            if *towns >= 5 {
                seat.first_five_tcs.get_or_insert(tick);
            }
            if *towns >= 6 {
                seat.first_six_tcs.get_or_insert(tick);
            }
        }
        emit |= opponents.iter().any(Option::is_some);
        if !emit {
            return Ok(None);
        }

        let mut census = vec![Census::default(); self.seats.len()];
        for (count, towns) in census.iter_mut().zip(towns) {
            count.town_centers = towns;
        }
        for unit in world.units().chain(
            buildings
                .iter()
                .flat_map(|b| b.garrison.iter().map(|g| g.unit)),
        ) {
            if let Some(count) = census.get_mut(usize::from(unit.player.0)) {
                count.count(unit);
            }
        }
        let players = world
            .players()
            .map(|player| {
                let i = usize::from(player.player.0);
                let seat = &mut self.seats[i];
                let count = census[i];
                if let Some(opponent) = opponents[i] {
                    seat.first_contact = Some(Contact {
                        tick,
                        opponent,
                        town_centers: count.town_centers,
                        technologies: seat.research.iter().filter(|time| time.is_some()).count()
                            as u32,
                        age: player.age,
                        villagers: count.villagers,
                        army: count.army,
                    });
                }
                let (food_upkeep_per_minute, food_upkeep_shortage) =
                    world.food_upkeep(player.player);
                PlayerSample {
                    player: player.player,
                    age: player.age,
                    age_ticks: seat.age_ticks,
                    stockpile: player.stockpile,
                    home_gold: gold::counts(world, &self.gold_sites[i]),
                    delivered: seat.delivered,
                    town_centers: count.town_centers,
                    villagers: count.villagers,
                    army: count.army,
                    food_upkeep_per_minute,
                    food_upkeep_shortage,
                    research: seat.research.clone(),
                    first_contact: seat.first_contact.clone(),
                    first_five_tcs: seat.first_five_tcs,
                    first_six_tcs: seat.first_six_tcs,
                }
            })
            .collect();
        Ok(Some(Sample { tick, players }))
    }
}
