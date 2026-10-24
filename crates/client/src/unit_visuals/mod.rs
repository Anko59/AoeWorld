//! Pure choices from authoritative state; no client damage or lifetime timers.
pub(crate) use aoe_assets::catalog::units::Pose;
use aoe_assets::catalog::units::SPRITES;
use aoe_core::FIXED_SUBUNITS_PER_TILE;
use aoe_protocol::{AnimalKind, AnimalState, AnimalTask, GameplayUnitState, UnitKind, UnitTask};

pub(crate) fn unit_pose(u: &GameplayUnitState, animal: Option<&AnimalState>) -> Pose {
    use Pose::*;
    if let Some(poses) = military_poses(u.kind) {
        return if u.hp == 0 {
            poses[3]
        } else if u.moving {
            poses[1]
        } else if u.task == UnitTask::Attacking {
            poses[2]
        } else {
            poses[0]
        };
    }
    if u.kind == UnitKind::FishingShip {
        return if u.moving {
            FishingShipWalk
        } else if u.task == UnitTask::Fishing {
            FishingShipWork
        } else {
            FishingShipStand
        };
    }
    if u.task == UnitTask::Fishing && !u.moving {
        return FisherWork;
    }
    if u.task == UnitTask::Farming && !u.moving && u.hp > 0 {
        return FarmerWork;
    }
    if u.hp == 0 {
        return match u.kind {
            UnitKind::Militia => MilitiaDie,
            UnitKind::FishingShip => unreachable!(),
            UnitKind::Villager => match u.death_hunt {
                Some(AnimalKind::Sheep) => ShepherdDie,
                Some(_) => HunterDie,
                None => VillagerDie,
            },
            _ => unreachable!(),
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
    if u.hunt_target.is_some() && u.task == UnitTask::Returning {
        return HunterCarry;
    }
    let sheep = animal.is_some_and(|a| a.kind == AnimalKind::Sheep);
    if u.hunt_target.is_some() && u.task == UnitTask::Moving {
        return if sheep { ShepherdWalk } else { HunterWalk };
    }
    if u.hunt_target.is_some() && u.task == UnitTask::Gathering {
        return if !u.moving {
            Butcher
        } else if sheep {
            ShepherdWalk
        } else {
            HunterWalk
        };
    }
    let carcass = animal.is_some_and(|a| a.hp == 0);
    // The work pose plays only while the hunter actually strikes or carves:
    // a unit waiting for its route or still out of reach stands.
    let working = !u.planning && (carcass || in_reach(u, animal));
    if u.task == UnitTask::Shepherding {
        return if u.moving {
            ShepherdWalk
        } else if !working {
            ShepherdStand
        } else if carcass {
            Butcher
        } else {
            ShepherdWork
        };
    }
    if u.task == UnitTask::Hunting {
        return if u.moving {
            HunterWalk
        } else if !working {
            HunterStand
        } else if carcass {
            Butcher
        } else {
            HunterAttack
        };
    }
    if u.task == UnitTask::Attacking || u.hunt_target.is_none() {
        VillagerAttack
    } else if sheep {
        ShepherdStand
    } else {
        HunterStand
    }
}

/// A shepherd walking to its sheep (or to a sheep carcass) is drawn as an
/// ordinary walking villager; the herding walk is for leading a living sheep.
pub(crate) fn walks_as_villager(u: &GameplayUnitState, animal: Option<&AnimalState>) -> bool {
    u.hunt_target.is_some()
        && u.task == UnitTask::Moving
        && animal.is_some_and(|a| a.kind == AnimalKind::Sheep)
}

/// The simulation's strike reach (`hunter_step`): one tile for a sheep, four
/// for bow hunting.
fn in_reach(u: &GameplayUnitState, animal: Option<&AnimalState>) -> bool {
    animal.is_some_and(|a| {
        let reach =
            if a.kind == AnimalKind::Sheep { 1 } else { 4 } * i64::from(FIXED_SUBUNITS_PER_TILE);
        let dx = i64::from(u.position.x) - i64::from(a.position.x);
        let dy = i64::from(u.position.y) - i64::from(a.position.y);
        dx * dx + dy * dy <= reach * reach
    })
}

fn military_poses(kind: UnitKind) -> Option<[Pose; 4]> {
    use Pose::*;
    Some(match kind {
        UnitKind::Archer => [ArcherStand, ArcherWalk, ArcherAttack, ArcherDie],
        UnitKind::Skirmisher => [
            SkirmisherStand,
            SkirmisherWalk,
            SkirmisherAttack,
            SkirmisherDie,
        ],
        UnitKind::Spearman => [SpearmanStand, SpearmanWalk, SpearmanAttack, SpearmanDie],
        UnitKind::ManAtArms => [ManAtArmsStand, ManAtArmsWalk, ManAtArmsAttack, ManAtArmsDie],
        UnitKind::Scout => [ScoutStand, ScoutWalk, ScoutAttack, ScoutDie],
        _ => return None,
    })
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
        (ArcherDie, ArcherDecay),
        (SkirmisherDie, SkirmisherDecay),
        (SpearmanDie, SpearmanDecay),
        (ManAtArmsDie, ManAtArmsDecay),
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
    let millis = u64::from(source.millis) * u64::from(source.sample_step());
    let stride = u64::from(source.stride.div_ceil(source.sample_step()));
    let phase = if let Some(died) = death {
        // The same native frame duration governs looping and one-shot playback.
        let elapsed = tick.saturating_sub(died.0).saturating_mul(1000) / u64::from(hz.max(1));
        let delay = decay_pair(pose)
            .filter(|(_, decay)| *decay == pose)
            .map_or(0, |(die, _)| {
                let s = SPRITES[die as usize];
                u64::from(s.millis) * u64::from(s.stride)
            });
        (elapsed.saturating_sub(delay) / millis).min(stride - 1) as u8
    } else {
        ((time.max(0.0) as u64 % (u64::from(source.stride) * u64::from(source.millis))) / millis)
            .min(stride - 1) as u8
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
    } else if resource && matches!(kind, UnitKind::Villager | UnitKind::FishingShip) {
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
