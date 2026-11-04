//! Owner-visible bounded squad offices, derived from authoritative membership.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SquadKind {
    Infantry,
    Archers,
    Cavalry,
    Siege,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SquadStatus {
    pub leader: EntityId,
    pub kind: SquadKind,
    pub size: u8,
    pub state: GeneralState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneralStatus {
    pub army_size: u16,
    pub state: GeneralState,
    pub training: bool,
    pub queued: u16,
    pub mix: [u8; 5],
    pub counts: [u16; 5],
    pub squads: [Option<SquadStatus>; 10],
}

impl GameWorld {
    pub fn general_status(&self, id: EntityId) -> Option<GeneralStatus> {
        let army = self.economy.generals.armies.get(&id)?;
        let mut counts = [0; 5];
        for r in army
            .members
            .iter()
            .filter_map(|&m| self.unit(m).filter(|u| u.hp > 0))
            .filter_map(|u| role(u.kind))
        {
            counts[r] += 1;
        }
        let mut squads = [None; 10];
        for (out, squad) in squads.iter_mut().zip(&army.squads) {
            let &leader = squad.members.first()?;
            let (fighting, moving) = squad.members.iter().filter_map(|&id| self.unit(id)).fold(
                (false, false),
                |(fight, moving), u| {
                    (
                        fight || u.attack_target.is_some(),
                        moving || u.moving || u.planning,
                    )
                },
            );
            let state = if fighting {
                GeneralState::Fighting
            } else if moving && army.state == GeneralState::Retreating {
                GeneralState::Retreating
            } else if moving {
                GeneralState::Marching
            } else {
                GeneralState::Idle
            };
            *out = Some(SquadStatus {
                leader,
                kind: [
                    SquadKind::Infantry,
                    SquadKind::Archers,
                    SquadKind::Cavalry,
                    SquadKind::Siege,
                ][squad.arm],
                size: squad.members.len() as u8,
                state,
            });
        }
        Some(GeneralStatus {
            army_size: army.members.len() as u16,
            state: army.state,
            training: army.training,
            queued: army.pending.len() as u16,
            mix: army.mix,
            counts,
            squads,
        })
    }

    pub(super) fn set_sergeant(&mut self, id: EntityId, enabled: bool) {
        if let Some(&index) = self.lookup.get(&id) {
            let unit = &mut self.units[index].state;
            if unit.sergeant != enabled {
                unit.sergeant = enabled;
                self.economy.generals.appearance_changes.insert(id);
            }
        }
    }
}
