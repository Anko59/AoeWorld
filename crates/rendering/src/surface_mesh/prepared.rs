//! View-local terrain demand and appearance, independent of actor animation.
use super::*;

/// Native geometry is immutable after loading. A new camera, terrain or
/// foundation snapshot needs a new plan; animation residency does not.
pub struct PreparedTerrain {
    surfaces: Vec<ProjectedSurfaceTriangle>,
    required: Vec<u32>,
    streamed_cliffs: bool,
}

impl PreparedTerrain {
    pub fn new(
        art: &mut GameArt,
        terrain: &[SceneTerrain],
        surfaces: &[ProjectedSurfaceTriangle],
        camera: SceneCamera,
    ) -> Self {
        // Density selection must precede material assignment. Keep the chosen
        // level and demand for the lifetime of this camera/ground snapshot.
        let mut required = scene_terrain_frames(art, terrain, camera);
        required.extend(scene_cliff_native_frames(art, surfaces));
        Self {
            required,
            ..Self::textured(art, surfaces)
        }
    }

    pub(crate) fn textured(art: &GameArt, surfaces: &[ProjectedSurfaceTriangle]) -> Self {
        let streamed_cliffs = art.environment.iter().any(|c| c.unit_id == 264);
        let mut surfaces = surfaces.to_vec();
        for face in &mut surfaces {
            // Cliff packets contain physical LRU addresses. Resolve just these
            // faces at submission, after uploads/evictions, including cold art.
            if !(streamed_cliffs && cliff(face)) {
                apply_terrain_textures(std::slice::from_mut(face), art);
            }
        }
        Self {
            surfaces,
            required: Vec::new(),
            streamed_cliffs,
        }
    }

    pub fn required(&self) -> &[u32] {
        &self.required
    }

    pub fn resident_bytes(&self) -> usize {
        self.surfaces.capacity() * size_of::<ProjectedSurfaceTriangle>()
            + self.required.capacity() * size_of::<u32>()
    }

    pub(crate) fn surfaces<'a>(
        &'a self,
        art: &'a GameArt,
    ) -> impl Iterator<Item = ProjectedSurfaceTriangle> + 'a {
        self.surfaces.iter().copied().map(|mut face| {
            if self.streamed_cliffs && cliff(&face) {
                apply_terrain_textures(std::slice::from_mut(&mut face), art);
            }
            face
        })
    }
}

fn cliff(face: &ProjectedSurfaceTriangle) -> bool {
    face.skirt && face.tint == 13
}
