use super::{
    Facing, GameUnit, GameWorld, GameWorldError, StoredUnit, UnitKind, UnitTask, unit_combat,
};
use aoe_core::{ChunkCoord, EntityId, PlayerId, WorldPosition};
use std::collections::VecDeque;

impl GameWorld {
    pub fn default_with_village(seed: aoe_core::Seed) -> (Self, EntityId) {
        let config = crate::movement_speed::cavalry_config(aoe_core::WorldConfig {
            seed,
            ..aoe_core::WorldConfig::default()
        });
        let mut world = Self::from_valid_config(config);
        let center = aoe_core::TileCoord::new(config.width_tiles / 2, config.height_tiles / 2);
        let id = world
            .initialize_village(PlayerId(0), center)
            .unwrap_or(EntityId(0));
        (world, id)
    }

    pub(crate) fn allocate_entity_id(&mut self) -> Result<EntityId, GameWorldError> {
        let id = EntityId(
            u32::try_from(self.next_entity_id).map_err(|_| GameWorldError::EntityIdExhausted)?,
        );
        self.next_entity_id += 1;
        self.combat.hp_quarters.push(0);
        Ok(id)
    }
    pub fn spawn_unit(
        &mut self,
        player: PlayerId,
        position: WorldPosition,
    ) -> Result<EntityId, GameWorldError> {
        self.spawn_kind(player, UnitKind::Scout, position)
    }
    pub fn spawn_villager(
        &mut self,
        player: PlayerId,
        position: WorldPosition,
    ) -> Result<EntityId, GameWorldError> {
        self.spawn_kind(player, UnitKind::Villager, position)
    }
    /// Construct a unit for a server-owned scenario. Gameplay clients train
    /// units through a production building and cannot call this constructor.
    pub fn spawn_kind(
        &mut self,
        player: PlayerId,
        kind: UnitKind,
        position: WorldPosition,
    ) -> Result<EntityId, GameWorldError> {
        if !self.config.valid_ground_position(position) {
            return Err(GameWorldError::InvalidPosition);
        }
        let passable = self
            .unit_tile_passable(kind, position.tile_floor(), &|| false)
            .map_err(GameWorldError::Environment)?;
        if !passable || self.economy.blocked_tiles.contains(&position.tile_floor()) {
            return Err(GameWorldError::InvalidPosition);
        }
        let id = self.allocate_entity_id()?;
        let state = GameUnit {
            id,
            player,
            position,
            previous_position: position,
            moving: false,
            planning: false,
            facing: Facing::South,
            kind,
            task: UnitTask::Idle,
            carried: None,
            hp: unit_combat(kind).hp + self.researched_hp_bonus(player, kind),
            max_hp: unit_combat(kind).hp + self.researched_hp_bonus(player, kind),
            attack_target: None,
            last_hit: None,
            death_tick: None,
            death_hunt: None,
            auto_scout: false,
            stance: if kind == UnitKind::BatteringRam {
                aoe_core::CombatStance::Defensive
            } else {
                Default::default()
            },
        };
        if let Some(player) = self.economy.players.get_mut(&player) {
            player.population_used = player.population_used.saturating_add(1);
        }
        self.insert_map_unit(state, 0);
        Ok(id)
    }

    pub(crate) fn insert_map_unit(&mut self, state: GameUnit, next_hit_tick: u64) {
        let (id, position) = (state.id, state.position);
        if state.kind == UnitKind::Monk {
            self.combat.healers.insert(id);
        }
        if super::combat::acquisition::autonomous_kind(state.kind) {
            self.combat.seekers.insert(id);
        }
        let bucket = ChunkCoord::from_position(position);
        let bucket_slot = self.chunks.entry(bucket).or_default().len();
        self.chunks.entry(bucket).or_default().push(id);
        self.lookup.insert(id, self.units.len());
        self.units.push(StoredUnit {
            formation: None,
            state,
            order: None,
            obstruction: None,
            bucket,
            bucket_slot,
            wildlife_slot: usize::MAX,
            route: VecDeque::new(),
            planner: None,
            last_movement_error: None,
            next_hit_tick,
            planning_since: 0,
            fast_route_tried: false,
            unreachable_memo: None,
        });
        let state = self.units[self.units.len() - 1].state;
        self.track_unit_cell(&state);
        self.idle_spacing.insert(id, state.position.tile_floor());
        self.update_unit_vision(id);
    }
}
