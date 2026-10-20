//! Pure choices from authoritative state; no client damage or lifetime timers.
pub(crate) use aoe_assets::catalog::units::Pose;
use aoe_assets::catalog::units::SPRITES;
use aoe_protocol::{AnimalKind, AnimalState, AnimalTask, GameplayUnitState, UnitKind, UnitTask};

pub(crate) fn unit_pose(u: &GameplayUnitState, animal: Option<&AnimalState>) -> Pose {
    use Pose::*;
    if u.hp == 0 {
        return match u.kind {
            UnitKind::Militia => MilitiaDie,
            UnitKind::Scout => ScoutDie,
            UnitKind::Villager => match u.death_hunt {
                Some(AnimalKind::Sheep) => ShepherdDie,
                Some(_) => HunterDie,
                None => VillagerDie,
            },
        };
    }
    if u.kind == UnitKind::Militia {
        return if u.moving {
            MilitiaWalk
        } else if u.task == UnitTask::Attacking {
            MilitiaAttack
        } else {
            MilitiaStand
        };
    }
    if u.kind == UnitKind::Scout {
        return ScoutAttack;
    }
    if u.hunt_target.is_some() && u.task == UnitTask::Returning {
        return HunterCarry;
    }
    if u.hunt_target.is_some() && u.task == UnitTask::Moving {
        return if animal.is_some_and(|a| a.kind == AnimalKind::Sheep) {
            ShepherdWalk
        } else {
            HunterWalk
        };
    }
    if u.hunt_target.is_some() && u.task == UnitTask::Gathering {
        return if !u.moving {
            Butcher
        } else if animal.is_some_and(|a| a.kind == AnimalKind::Sheep) {
            ShepherdWalk
        } else {
            HunterWalk
        };
    }
    if u.task == UnitTask::Shepherding {
        return if u.moving {
            ShepherdWalk
        } else if animal.is_some_and(|a| a.hp == 0) {
            Butcher
        } else {
            ShepherdWork
        };
    }
    if u.task == UnitTask::Hunting {
        return if u.moving {
            HunterWalk
        } else if animal.is_some_and(|a| a.hp == 0) {
            Butcher
        } else {
            HunterAttack
        };
    }
    VillagerAttack
}

pub(crate) fn animal_pose(a: &AnimalState) -> Pose {
    use Pose::*;
    if a.kind == AnimalKind::Sheep && a.owner.is_none() {
        return if a.hp == 0 {
            NeutralSheepDie
        } else if a.moving {
            NeutralSheepWalk
        } else {
            NeutralSheepStand
        };
    }
    match a.kind {
        AnimalKind::Sheep => {
            if a.hp == 0 {
                SheepDie
            } else if a.moving {
                SheepWalk
            } else {
                SheepStand
            }
        }
        AnimalKind::Deer => {
            if a.hp == 0 {
                DeerDie
            } else if a.moving {
                DeerWalk
            } else {
                DeerStand
            }
        }
        AnimalKind::Boar => {
            if a.hp == 0 {
                BoarDie
            } else if a.moving {
                BoarWalk
            } else if a.task == AnimalTask::Attacking {
                BoarAttack
            } else {
                BoarStand
            }
        }
        AnimalKind::Wolf => {
            if a.hp == 0 {
                WolfDie
            } else if a.moving {
                if a.task == AnimalTask::Attacking {
                    WolfRun
                } else {
                    WolfWalk
                }
            } else if a.task == AnimalTask::Attacking {
                WolfAttack
            } else {
                WolfStand
            }
        }
    }
}

fn decay_pair(pose: Pose) -> Option<(Pose, Pose)> {
    use Pose::*;
    [
        (MilitiaDie, MilitiaDecay),
        (VillagerDie, VillagerDecay),
        (SheepDie, SheepDecay),
        (DeerDie, DeerDecay),
        (BoarDie, BoarDecay),
        (WolfDie, WolfDecay),
        (ScoutDie, ScoutDecay),
        (HunterDie, HunterDecay),
        (ShepherdDie, ShepherdDecay),
        (NeutralSheepDie, NeutralSheepDecay),
    ]
    .into_iter()
    .find(|(die, decay)| pose == *die || pose == *decay)
}
pub(crate) fn lifecycle_pose(pose: Pose, died: Option<aoe_core::Tick>, tick: u64, hz: u32) -> Pose {
    let Some((die, decay)) = decay_pair(pose) else {
        return pose;
    };
    let Some(died) = died else {
        return pose;
    };
    let source = SPRITES[die as usize];
    if tick.saturating_sub(died.0).saturating_mul(1000)
        >= u64::from(hz.max(1)) * u64::from(source.millis) * u64::from(source.stride)
    {
        decay
    } else {
        die
    }
}
pub(crate) fn packed_frame(
    pose: Pose,
    facing: u8,
    time: f64,
    death: Option<aoe_core::Tick>,
    tick: u64,
    hz: u32,
) -> u8 {
    let source = SPRITES[pose as usize];
    let phase = if let Some(died) = death {
        // The same native frame duration governs looping and one-shot playback.
        let elapsed = tick.saturating_sub(died.0).saturating_mul(1000) / u64::from(hz.max(1));
        let delay = decay_pair(pose)
            .filter(|(_, decay)| *decay == pose)
            .map_or(0, |(die, _)| {
                let s = SPRITES[die as usize];
                u64::from(s.millis) * u64::from(s.stride)
            });
        (elapsed.saturating_sub(delay) / u64::from(source.millis)).min(u64::from(source.stride - 1))
            as u8
    } else {
        (time.max(0.0) as u64 / u64::from(source.millis) % u64::from(source.stride)) as u8
    };
    (facing % 8) | (phase << 3)
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ClickOrder {
    Move,
    Gather,
    Attack,
    Hunt,
}
pub(crate) fn click_order(
    kind: UnitKind,
    move_only: bool,
    animal: bool,
    enemy: bool,
    resource: bool,
) -> ClickOrder {
    if move_only {
        ClickOrder::Move
    } else if animal && kind == UnitKind::Villager {
        ClickOrder::Hunt
    } else if animal || enemy {
        ClickOrder::Attack
    } else if resource && kind == UnitKind::Villager {
        ClickOrder::Gather
    } else {
        ClickOrder::Move
    }
}

/// Native anchors may lie below the image; their small ground target is usable.
pub(crate) fn actor_hit(bounds: [f64; 4], foot: [f64; 2], point: [f64; 2], zoom: f64) -> bool {
    let dx = point[0] - foot[0];
    let dy = point[1] - foot[1];
    (point[0] >= bounds[0]
        && point[0] <= bounds[2]
        && point[1] >= bounds[1]
        && point[1] <= bounds[3])
        || dx * dx + dy * dy <= (5.0 * zoom).powi(2)
}
