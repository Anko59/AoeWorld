//! Integer observations of executed CPU work, present only in benchmark builds.
use std::cell::Cell;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Counter {
    TerrainDemandTiles,
    TexturedTriangles,
    CliffFaces,
    ProjectedTiles,
    SpriteInputs,
    SceneBuilds,
}
pub const NAMES: &str = "terrain.demand_tiles,terrain.textured_triangles,terrain.cliff_faces,terrain.projected_tiles,sprites.inputs,terrain.scene_builds";
thread_local! {
    static COUNTS: Cell<[u64; 6]> = const { Cell::new([0; 6]) };
}
pub fn add(counter: Counter, amount: usize) {
    COUNTS.with(|counts| {
        let mut values = counts.get();
        values[counter as usize] = values[counter as usize]
            .checked_add(amount as u64)
            .expect("frame cost overflow");
        counts.set(values);
    });
}
pub fn take() -> Vec<u64> {
    COUNTS.with(|counts| counts.replace([0; 6]).to_vec())
}
