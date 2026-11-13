use super::{
    Result,
    stats::{Fight, Movement},
};
use aoe_core::{EntityId, Formation, PlayerId, TileCoord, WorldPosition};
use aoe_simulation::{GameWorld, StewardSettings, UnitKind};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OrderPath {
    Single,
    Group,
    General,
    Steward,
}
impl OrderPath {
    pub const ALL: [Self; 4] = [Self::Single, Self::Group, Self::General, Self::Steward];
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Group => "formation",
            Self::General => "general-squads",
            Self::Steward => "steward-transfer",
        }
    }
}
pub(super) fn at(x: i32, y: i32) -> Result<WorldPosition> {
    Ok(WorldPosition::from_tile_center(TileCoord::new(x, y))?)
}
pub(super) fn distance(a: WorldPosition, b: WorldPosition) -> u64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    (x * x + y * y).isqrt() as u64
}
pub(super) struct Scene {
    pub world: GameWorld,
    pub ids: Vec<EntityId>,
    pub goal: WorldPosition,
    pub path: OrderPath,
    pub controller: Option<EntityId>,
    pub attack_move: bool,
    pub limit: u64,
}
pub(super) fn prepare(name: &str, path: OrderPath) -> Result<Scene> {
    let world = aoe_simulation::terrain_fixtures::measurement_world(name);
    let (start, goal, limit) = match name {
        "quality-march" | "long-bridges-40" => ((35, 103), (610, 110), 24_000),
        "long" => ((35, 90), (1045, 100), 40_000),
        "army-pass" => ((70, 145), (220, 150), 12_000),
        "river" => ((35, 43), (145, 48), 8_000),
        _ => return Err("unknown quality march".into()),
    };
    populate(world, start, goal, path, UnitKind::Militia, limit)
}
pub(super) fn populate(
    mut world: GameWorld,
    start: (i32, i32),
    goal: (i32, i32),
    path: OrderPath,
    kind: UnitKind,
    limit: u64,
) -> Result<Scene> {
    let mut controller = None;
    // Identical Town Center obstacles for all order paths. The autonomous
    // transfer destination is exactly `goal`, three tiles before its centre.
    let home = super::setup::town_near(&mut world, (start.0 - 8, start.1 - 8))?;
    let dest = super::setup::town_near(&mut world, (goal.0 + 1, goal.1 + 1))?;
    aoe_simulation::terrain_fixtures::measurement_starting_kit(
        &mut world,
        home,
        TileCoord::new(start.0, start.1),
    )?;
    let footprint = world
        .building(dest)
        .ok_or("destination Town Center is missing")?
        .footprint;
    let center = (footprint.min.x + 2, footprint.min.y + 2);
    let goal = (
        center.0 + (start.0 - center.0).signum() * 3,
        center.1 + (start.1 - center.1).signum() * 3,
    );
    if path == OrderPath::Steward {
        for town in [home, dest] {
            let b = world.building(town).ok_or("setup Town Center is missing")?;
            let p = at(b.footprint.min.x - 1, b.footprint.min.y + 1)?;
            let tile = p.tile_floor();
            let id = super::setup::spawn_near(&mut world, UnitKind::Steward, (tile.x, tile.y))?;
            world.issue_garrison(PlayerId(0), id, town)?;
            if town == home {
                controller = Some(id);
            }
        }
        for _ in 0..80 {
            world.advance();
        }
    }
    // Forty exercises more than one General squad, rather than relabelling a
    // single twenty-member formation as an army. Other group paths match it.
    let count = if path == OrderPath::Single { 1 } else { 40 };
    let mut ids = Vec::new();
    for n in 0..count {
        let kind = if path == OrderPath::Steward {
            UnitKind::Villager
        } else {
            kind
        };
        ids.push(super::setup::spawn_near(
            &mut world,
            kind,
            (start.0 + n % 5, start.1 + n / 5),
        )?);
    }
    if path == OrderPath::General {
        controller = Some(super::setup::spawn_near(
            &mut world,
            UnitKind::General,
            (start.0 - 1, start.1 - 1),
        )?);
    }
    if path == OrderPath::General
        && !ids
            .iter()
            .all(|id| world.general_controller(*id) == controller)
    {
        return Err("General did not adopt the complete quality cohort".into());
    }
    if path == OrderPath::Steward {
        // Adoption uses real accepted gather jobs. This is setup, before the
        // measured cap change that causes the Steward's own transfer orders.
        super::setup::assign_steward_jobs(&mut world, &ids, start)?;
        for _ in 0..40 {
            world.advance();
        }
        if !ids
            .iter()
            .all(|id| world.steward_controller(*id) == controller)
        {
            return Err("Steward did not adopt the complete quality cohort".into());
        }
    }
    Ok(Scene {
        world,
        ids,
        goal: at(goal.0, goal.1)?,
        path,
        controller,
        attack_move: false,
        limit,
    })
}
impl Scene {
    pub fn issue(&mut self) -> Result<()> {
        let p = PlayerId(0);
        match self.path {
            OrderPath::Single => {
                if self.attack_move {
                    for &id in &self.ids {
                        self.world.issue_formation_move(
                            p,
                            &[id],
                            Formation::Line,
                            self.goal,
                            true,
                            false,
                        )?;
                    }
                } else {
                    let id = *self.ids.first().ok_or("quality cohort is empty")?;
                    self.world.issue_move(id, self.goal)?;
                }
            }
            OrderPath::Group => {
                self.world.issue_formation_move(
                    p,
                    &self.ids,
                    Formation::Line,
                    self.goal,
                    self.attack_move,
                    false,
                )?;
            }
            OrderPath::General => {
                let g = self
                    .controller
                    .ok_or("quality order controller is missing")?;
                self.world.general_player_order(p, &[g], Some(self.goal));
                self.world.issue_move(g, self.goal)?;
            }
            OrderPath::Steward => {
                self.world.issue_steward_settings(
                    p,
                    self.controller
                        .ok_or("quality order controller is missing")?,
                    StewardSettings {
                        cap: 0,
                        ..StewardSettings::default()
                    },
                )?;
            }
        }
        Ok(())
    }
    pub fn measure(mut self, name: &str, video: Option<&std::path::Path>) -> Result<Value> {
        let units: Vec<_> = self
            .ids
            .iter()
            .map(|&id| {
                self.world
                    .unit(id)
                    .ok_or_else(|| format!("quality cohort actor {} is missing", id.0))
            })
            .collect::<std::result::Result<_, _>>()?;
        let kind = units.first().ok_or("quality cohort is empty")?.kind;
        let starts: Vec<_> = units.iter().map(|unit| unit.position).collect();
        let mut bounds: Vec<_> = units
            .iter()
            .map(|unit| {
                let (speed, denom) = self.world.movement_speed(unit.kind, unit.player);
                (distance(unit.position, self.goal).saturating_sub(2 * 1024) * denom)
                    .div_ceil(speed)
                    .max(1)
            })
            .collect();
        let squad_count = self
            .controller
            .and_then(|id| self.world.general_status(id))
            .map(|status| status.squads.iter().flatten().count());
        let mut movement = Movement::new(bounds.clone());
        let mut fight = Fight::new(self.ids.len());
        let mut goals = vec![None; self.ids.len()];
        let mut missing_slots = 0_u64;
        let mut admitted = vec![false; self.ids.len()];
        self.world.set_damage_observation(true);
        self.issue()?;
        let initial_tick = self.world.tick().0;
        let hz = u64::from(self.world.config().tick_hz);
        let mut elapsed = 0;
        for tick in 1..=self.limit {
            elapsed = tick;
            self.world.advance();
            for (n, &id) in self.ids.iter().enumerate() {
                let slot = self.world.formation_destination_slot(id);
                if !self.attack_move
                    && let Some(slot) = slot
                {
                    goals[n] = Some(slot);
                    let unit = self
                        .world
                        .unit(id)
                        .ok_or("formation slot actor is missing")?;
                    let (speed, denom) = self.world.movement_speed(unit.kind, unit.player);
                    bounds[n] = (distance(starts[n], slot).saturating_sub(2 * 1024) * denom)
                        .div_ceil(speed)
                        .max(1);
                    movement.bound(n, bounds[n]);
                }
                let unit = self.world.unit(id);
                admitted[n] |= unit.is_some_and(|u| u.moving || u.planning) || slot.is_some();
                let goal = goals[n].unwrap_or(self.goal);
                let position = unit.map(|u| u.position).unwrap_or(starts[n]);
                let arrived =
                    unit.is_some_and(|u| u.hp > 0) && distance(position, goal) <= 2 * 1024;
                let shape = self
                    .world
                    .formation_slot(id)
                    .map(|slot| distance(position, slot));
                if matches!(self.path, OrderPath::Group | OrderPath::General) && shape.is_none() {
                    missing_slots += 1;
                }
                movement.sample(
                    n,
                    self.world.movement_obstructed(id),
                    distance(position, starts[n]),
                    shape,
                );
                if arrived {
                    movement.arrive(n, tick);
                }
            }
            for hit in self.world.take_damage_observations() {
                if let Some(n) = self.ids.iter().position(|&id| id == hit.attacker) {
                    fight.hit(n, tick, u64::from(hit.amount_quarters));
                }
                if self.ids.contains(&hit.target) {
                    fight.received(u64::from(hit.amount_quarters));
                }
            }
            let finished = !self.attack_move && movement.all_arrived();
            if let Some(directory) = video
                && (tick.is_multiple_of(100) || tick == self.limit || finished)
            {
                super::video::frame(
                    directory,
                    super::video::FrameSummary {
                        scenario: name,
                        path: self.path.name(),
                        arrived: movement.arrived_count(),
                    },
                    tick,
                    &self.world,
                    &self.ids,
                    &starts,
                    self.goal,
                )?;
            }
            if finished {
                break;
            }
        }
        Ok(
            json!({"scenario": name, "order_path": self.path.name(), "seed": self.world.config().seed.0,
            "general_squads": squad_count, "setup_starting_resource_nodes": self.world.starting_resources().len(),
            "arrival_target": if self.attack_move { "requested-objective-radius-2-tiles" } else { "assigned-destination-slot-radius-2-tiles" },
            "tick_hz": hz, "deadline_ticks": self.limit, "observed_ticks": elapsed, "setup_ticks": initial_tick,
            "requested_destination": self.goal,
            "unit_kind": kind, "straight_line_lower_bound_ticks_min": bounds.iter().min(),
            "straight_line_lower_bound_ticks_max": bounds.iter().max(),
            "units_with_observed_movement_order": admitted.into_iter().filter(|v| *v).count(),
            "missing_slot_unit_ticks": missing_slots,
            "movement": movement.finish(),
            "fight": self.attack_move.then(|| fight.finish(self.limit,hz)) }),
        )
    }
}
