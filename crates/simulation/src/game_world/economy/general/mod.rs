//! Deterministic army command; execution uses ordinary routes and combat orders.
use crate::{GameUnit, GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, WorldPosition};
use std::collections::{BTreeMap, BTreeSet};
mod decisions;
mod fight;
mod formation;
mod hash;
mod orders;
mod reinforcement;
mod squads;
mod status;
pub use status::{GeneralStatus, SquadKind, SquadStatus};
mod stances;
mod survey;
use stances::StanceOverride;

pub const GENERAL_ARMY_LIMIT: usize = 200;
/// Tiles around a General within which he commands soldiers and picks buildings.
pub const GENERAL_INFLUENCE_TILES: i64 = 50;
/// Infantry:archers:heavy cavalry:siege:scouts a new General aims for.
pub const DEFAULT_MIX: [u8; 5] = [35, 30, 20, 10, 5];
pub const GENERAL_CADENCE: u64 = 10;
pub const GENERAL_ADMISSIONS: usize = 8;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneralState {
    Idle,
    Marching,
    Fighting,
    Retreating,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GeneralWork {
    pub generals: usize,
    pub unit_visits: usize,
    pub building_visits: usize,
    pub orders: usize,
}
#[derive(Debug)]
pub(crate) struct Army {
    pub castle: Option<EntityId>,
    pub home: WorldPosition,
    pub pending: BTreeMap<EntityId, UnitKind>,
    pub members: BTreeSet<EntityId>,
    pub squads: Vec<squads::Squad>,
    /// Stable orientation for retries of the same army destination.
    pub layout: Option<(WorldPosition, WorldPosition)>,
    /// Shared march pace, refreshed once per admitted army decision.
    pub pace: (u64, u64),
    pub destination: Option<WorldPosition>,
    pub state: GeneralState,
    pub micro_after: usize,
    /// Next first fighting group, including the commander, for shared searches.
    pub fight_after: usize,
    pub scan: (usize, usize),
    /// Next bounded slice of each production-kind index.
    pub production_scan: [usize; 5],
    pub retreat_until: u64,
    pub routed: Option<WorldPosition>,
    /// The player is moving the General alone; the army holds and is not led.
    pub solo: bool,
    /// The player pressed Train: recruits are bought until the army is full.
    pub training: bool,
    /// Unit-mix sliders per role; a role at zero is never bought.
    pub mix: [u8; 5],
    /// The enemy each fighting group of `fight::FIGHT_GROUP` soldiers is sent against.
    pub fight: [Option<EntityId>; fight::FIGHT_GROUPS],
    stances: [Option<StanceOverride>; GENERAL_ARMY_LIMIT + 1],
}
#[derive(Debug, Default)]
pub(crate) struct Manager {
    pub armies: crate::game_world::changes::Reports<Army>,
    pub controlled: crate::game_world::changes::Reports<EntityId>,
    // Only an empty building queue can be reserved. Its front is the recruit.
    pub recruits: BTreeMap<EntityId, (EntityId, UnitKind)>,
    /// Soldiers the player took out of an army by ordering them directly, with
    /// the tick of that order. No census adopts them; only a fresh order of
    /// their own player's General readmits them (see `general_player_order`).
    pub manual: BTreeMap<EntityId, u64>,
    pub cursor: Option<EntityId>,
    pub work: GeneralWork,
    pub appearance_changes: BTreeSet<EntityId>,
}
pub(super) fn role(kind: UnitKind) -> Option<usize> {
    Some(match kind {
        UnitKind::Militia
        | UnitKind::ManAtArms
        | UnitKind::LongSwordsman
        | UnitKind::Spearman
        | UnitKind::Pikeman
        | UnitKind::TwoHandedSwordsman
        | UnitKind::Champion
        | UnitKind::Halberdier => 0,
        UnitKind::Archer
        | UnitKind::Crossbowman
        | UnitKind::Arbalester
        | UnitKind::Skirmisher
        | UnitKind::EliteSkirmisher => 1,
        UnitKind::Knight | UnitKind::Cavalier | UnitKind::Paladin => 2,
        kind if kind.is_siege() => 3,
        UnitKind::Scout | UnitKind::LightCavalry | UnitKind::Hussar => 4,
        _ => return None,
    })
}
/// Whether `p` lies inside a General's influence sphere centred on `centre`.
pub(super) fn in_influence(centre: WorldPosition, p: WorldPosition) -> bool {
    distance(centre, p) <= (GENERAL_INFLUENCE_TILES * 1024).pow(2)
}
pub(super) fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    x * x + y * y
}
impl GameWorld {
    pub(crate) fn register_general(&mut self, id: EntityId) {
        let Some(u) = self.unit(id) else { return };
        let castle = self
            .player_buildings_of(u.player, crate::BuildingKind::Castle)
            .take(128)
            .filter(|b| b.is_complete() && b.hp > 0)
            .min_by_key(|b| {
                (
                    distance(
                        u.position,
                        super::placement::position(b.footprint.min).unwrap_or(u.position),
                    ),
                    b.id,
                )
            })
            .map(|b| b.id);
        let home = castle.map_or(u.position, |c| self.general_retreat_home(c, u.position));
        self.economy.generals.armies.insert(
            id,
            Army {
                castle,
                home,
                pending: BTreeMap::new(),
                members: BTreeSet::new(),
                squads: Vec::new(),
                layout: None,
                pace: (0, 1),
                destination: None,
                state: GeneralState::Idle,
                micro_after: 0,
                fight_after: 0,
                scan: (0, 0),
                production_scan: [0; 5],
                retreat_until: 0,
                routed: None,
                solo: false,
                training: false,
                mix: DEFAULT_MIX,
                fight: [None; fight::FIGHT_GROUPS],
                stances: [None; GENERAL_ARMY_LIMIT + 1],
            },
        );
        let mut work = GeneralWork::default();
        let _ = self.general_survey(id, &mut work);
    }
    pub(crate) fn bind_general_castle(&mut self, id: EntityId, castle: EntityId) {
        let Some(u) = self.unit(id) else { return };
        if let Some(a) = self.economy.generals.armies.get_mut(&id) {
            a.castle = Some(castle);
            a.home = u.position;
        }
    }
    fn general_retreat_home(&self, castle: EntityId, fallback: WorldPosition) -> WorldPosition {
        let Some(b) = self.building(castle) else {
            return fallback;
        };
        super::placement::perimeter(b.footprint)
            .into_iter()
            .filter_map(|tile| super::placement::position(tile).ok())
            .filter(|&p| {
                self.config.valid_ground_position(p)
                    && !self.economy.blocked_tiles.contains(&p.tile_floor())
                    && self
                        .unit_tile_passable(UnitKind::General, p.tile_floor(), &|| false)
                        .unwrap_or(false)
            })
            .min_by_key(|&p| (distance(p, fallback), p.x, p.y))
            .unwrap_or(fallback)
    }
    pub(crate) fn general_fighting(&self, g: EntityId) -> bool {
        self.economy
            .generals
            .armies
            .get(&g)
            .is_some_and(|a| a.state == GeneralState::Fighting)
    }

    pub fn general_controller(&self, id: EntityId) -> Option<EntityId> {
        self.economy.generals.controlled.get(&id).copied()
    }
    #[inline]
    pub(crate) fn is_general_follower(&self, unit: EntityId, general: EntityId) -> bool {
        self.economy.generals.controlled.get(&unit) == Some(&general)
    }
    /// Soldiers currently detached from every army by a player order.
    pub fn general_detached_count(&self) -> usize {
        self.economy.generals.manual.len()
    }
    pub fn general_castle(&self, id: EntityId) -> Option<EntityId> {
        self.economy.generals.armies.get(&id)?.castle
    }
    pub fn general_work(&self) -> GeneralWork {
        self.economy.generals.work
    }
    pub(crate) fn forget_general(&mut self, id: EntityId) {
        self.economy.generals.manual.remove(&id);
        self.restore_general_stances(id);
        if let Some(g) = self.general_controller(id) {
            self.restore_general_stance(g, id);
        }
        if let Some(army) = self.economy.generals.armies.remove(&id) {
            for member in army.members {
                self.set_sergeant(member, false);
                self.economy.generals.controlled.remove(&member);
            }
            for building in army.pending.keys() {
                self.economy.generals.recruits.remove(building);
            }
        }
        if let Some(g) = self.economy.generals.controlled.remove(&id)
            && let Some(army) = self.economy.generals.armies.get_mut(&g)
        {
            army.members.remove(&id);
            let promoted = army.remove_from_squads(id);
            army.routed = None;
            if let Some(id) = promoted {
                self.set_sergeant(id, true);
            }
        }
    }
    pub(crate) fn stop_general(&mut self, id: EntityId) {
        self.restore_general_stances(id);
        let Some(a) = self.economy.generals.armies.get_mut(&id) else {
            return;
        };
        a.destination = None;
        a.routed = None;
        a.layout = None;
        a.solo = false;
        a.state = GeneralState::Idle;
        a.retreat_until = 0;
        let mut members = [None; GENERAL_ARMY_LIMIT];
        for (slot, &id) in members.iter_mut().zip(&a.members) {
            *slot = Some(id);
        }
        for member in members.into_iter().flatten() {
            if let Some(&index) = self.lookup.get(&member) {
                self.stop_unit_orders(index);
            }
        }
    }
    pub(crate) fn general_march(&mut self, id: EntityId, destination: WorldPosition) {
        if let Some(a) = self.economy.generals.armies.get_mut(&id) {
            a.destination = Some(destination);
            a.routed = None;
            a.layout = None;
            a.solo = false;
        }
    }
    pub(super) fn claim_general_unit(&mut self, g: EntityId, u: GameUnit) {
        if self.vassal_controller(u.id).is_some()
            || role(u.kind).is_none()
            || u.hp == 0
            || self.embarked(u.id)
            || self.economy.generals.controlled.contains_key(&u.id)
        {
            return;
        }

        let mut promotion = None;
        if let Some(a) = self.economy.generals.armies.get_mut(&g)
            && a.members.len() + a.pending.len() < GENERAL_ARMY_LIMIT
            && a.room_for(u.kind)
        {
            promotion = a.assign_squad(u.id, u.kind);
            a.members.insert(u.id);
            a.routed = None;
            self.economy.generals.controlled.insert(u.id, g);
        }
        if let Some((old, new)) = promotion {
            if let Some(old) = old {
                self.set_sergeant(old, false);
            }
            self.set_sergeant(new, true);
        }
    }
    pub(crate) fn advance_generals(&mut self, changed: &mut Vec<EntityId>) {
        changed.extend(std::mem::take(
            &mut self.economy.generals.appearance_changes,
        ));
        if self.economy.generals.armies.is_empty() {
            return;
        }
        let mut work = GeneralWork::default();
        if self.tick.0.is_multiple_of(GENERAL_CADENCE) {
            use std::ops::Bound::{Excluded, Unbounded};
            let cursor = self.economy.generals.cursor.unwrap_or(EntityId(0));
            let ids: Vec<_> = self
                .economy
                .generals
                .armies
                .range((Excluded(cursor), Unbounded))
                .chain(self.economy.generals.armies.range(..=cursor))
                .map(|(&id, _)| id)
                .take(GENERAL_ADMISSIONS)
                .collect();
            for id in ids {
                self.economy.generals.cursor = Some(id);
                let Some(u) = self.unit(id).filter(|u| u.hp > 0 && !self.embarked(id)) else {
                    continue;
                };
                work.generals += 1;
                if let Some(survey) = self.general_survey(id, &mut work) {
                    self.general_decide(u, survey, changed, &mut work);
                    self.general_reinforce(id, u.player, &mut work);
                }
                changed.push(id);
            }
        }
        changed.extend(std::mem::take(
            &mut self.economy.generals.appearance_changes,
        ));
        self.economy.generals.work = work;
    }
}
