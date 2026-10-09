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
