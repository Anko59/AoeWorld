//! Compile-time startup view: descriptive catalog prose is not runtime payload.
use super::{
    AssetRole, OPTIONAL_RESOURCE_SOURCES, OPTIONAL_TERRAIN_SOURCES, REQUIRED_RENDER_SOURCES,
    SpriteSource, TerrainFrameTopology,
};

#[derive(Clone, Copy)]
pub struct RuntimeSpriteSource {
    pub role: AssetRole,
    pub archive: &'static str,
    pub id: u32,
    pub frames: u32,
    pub terrain_topology: Option<TerrainFrameTopology>,
}

pub const PLAYER_ROLES: [AssetRole; 11] = [
    AssetRole::CavalryStanding,
    AssetRole::CavalryWalking,
    AssetRole::VillagerStanding,
    AssetRole::VillagerWalking,
    AssetRole::VillagerChopping,
    AssetRole::VillagerCarryingWood,
    AssetRole::VillagerForaging,
    AssetRole::VillagerCarryingFood,
    AssetRole::VillagerCarryingGold,
    AssetRole::VillagerCarryingStone,
    AssetRole::VillagerMiningStone,
];

pub fn actor_maskable(index: usize) -> bool {
    let neutral = super::units::Pose::NeutralSheepStand as usize;
    !(neutral..neutral + 4).contains(&index)
}

impl RuntimeSpriteSource {
    pub fn manifest_source(self) -> String {
        format!("{}:[32, 112, 108, 115]:{}", self.archive, self.id)
    }
}

const fn project<const N: usize>(sources: [SpriteSource; N]) -> [RuntimeSpriteSource; N] {
    let mut result = [RuntimeSpriteSource {
        role: AssetRole::CavalryWalking,
        archive: "",
        id: 0,
        frames: 0,
        terrain_topology: None,
    }; N];
    let mut index = 0;
    while index < N {
        let source = sources[index];
        result[index] = RuntimeSpriteSource {
            role: source.role,
            archive: source.archive,
            id: source.id,
            frames: source.frames,
            terrain_topology: source.terrain_topology,
        };
        index += 1;
    }
    result
}

pub const REQUIRED: [RuntimeSpriteSource; REQUIRED_RENDER_SOURCES.len()] =
    project(REQUIRED_RENDER_SOURCES);
pub const OPTIONAL_RESOURCES: [RuntimeSpriteSource; OPTIONAL_RESOURCE_SOURCES.len()] =
    project(OPTIONAL_RESOURCE_SOURCES);
pub const OPTIONAL_TERRAIN: [RuntimeSpriteSource; OPTIONAL_TERRAIN_SOURCES.len()] =
    project(OPTIONAL_TERRAIN_SOURCES);

/// slot, source ID, complete prefix, selected-cell stride for ambient fish art.
pub const FISHING: [(usize, u32, u32, usize); 7] = [
    (0, 3549, 34, 2),
    (1, 1910, 49, 7),
    (2, 1911, 49, 7),
    (3, 1912, 49, 7),
    (4, 1913, 49, 7),
    (5, 420, 30, 5),
    (9, 446, 5, 1),
];

/// Keep standing wall damage cells and sample the ambient Dock wake. The
/// 60-cell, 150ms native wake keeps its nine-second loop at 30 cells, 300ms.
/// Castle Mill/Blacksmith ambient layers keep two poses; their large native
/// rectangles otherwise exhaust the object pages. Construction stays complete.
/// Damage graphics use unit ID -1 and retain two samples of their native loop.
pub fn building_frame_step(unit_id: i32, frames: u16, angles: u16, seconds: f32) -> usize {
    if unit_id == -1 && frames > 0 {
        usize::from(frames.div_ceil(2))
    } else if unit_id == 72 && frames > 0 {
        usize::from(frames)
    } else if matches!(unit_id, 130 | 18) && angles == 1 && frames > 1 && seconds > 0.0 {
        usize::from(frames.div_ceil(2))
    } else if matches!(unit_id, 45 | 133 | 47)
        && angles == 1
        && frames > 1
        && frames.is_multiple_of(2)
        && seconds > 0.0
        && seconds < 0.2
    {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_projection_preserves_every_loaded_field_and_semantic_order() {
        for (full, compact) in [
            (REQUIRED_RENDER_SOURCES.as_slice(), REQUIRED.as_slice()),
            (
                OPTIONAL_RESOURCE_SOURCES.as_slice(),
                OPTIONAL_RESOURCES.as_slice(),
            ),
            (
                OPTIONAL_TERRAIN_SOURCES.as_slice(),
                OPTIONAL_TERRAIN.as_slice(),
            ),
        ] {
            assert_eq!(full.len(), compact.len());
            for (source, runtime) in full.iter().zip(compact) {
                assert_eq!(source.role, runtime.role);
                assert_eq!(source.frames, runtime.frames);
                assert_eq!(source.terrain_topology, runtime.terrain_topology);
                assert_eq!(source.manifest_source(), runtime.manifest_source());
            }
        }
    }
}
