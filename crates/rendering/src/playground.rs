//! Local AoE II atlas rendering through the shared WebGPU sprite pipeline.
use crate::{Counters, Renderer, web::Sprite};

#[path = "playground/atlas.rs"]
mod atlas;
pub use atlas::{
    AtlasAddress, GAME_ATLAS_BYTES, GAME_ATLAS_PAGE_BYTES, GAME_ATLAS_PAGES, GAME_ATLAS_SIDE,
    TerrainTopology,
};

const TREE_VISUAL_VARIANTS: [usize; 40] = [
    0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13, 0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13, 0, 1, 2, 4, 6, 7, 9,
    10, 11, 12, 13, 0, 1, 2, 4, 3, 5, 8,
];

/// Selects the reviewed resource frame shared by rendering and viewport culling.
pub fn resource_frame_index(kind: u8, variant: u8, frame_count: usize) -> Option<usize> {
    if frame_count == 0 {
        return None;
    }
    Some(if kind == 1 && frame_count >= 14 {
        TREE_VISUAL_VARIANTS[usize::from(variant) % TREE_VISUAL_VARIANTS.len()]
    } else {
        usize::from(variant) % frame_count
    })
}

#[derive(Clone, Copy)]
pub struct GameFrame {
    pub atlas: AtlasAddress,
    pub size: [f32; 2],
    pub anchor: [f32; 2],
}

pub struct GameArt {
    pub walking: Vec<GameFrame>,
    pub standing: Vec<GameFrame>,
    pub grass: Vec<GameFrame>,
    /// Local terrain groups in this order: temperate grass, dry grass, dirt,
    /// sand, legacy rock (unused for natural surfaces), water, and optional
    /// forest accents. Procedural scene materials reuse dirt/water detail.
    /// Map binding stays in the client so the renderer remains independent
    /// from geographic map contracts.
    pub terrain: [Vec<GameFrame>; 7],
    /// Authored sheet topology; never infer a repeating sheet from frame count.
    pub terrain_topology: [Option<TerrainTopology>; 7],
    /// Resource groups in map wire order: food, wood, gold, then stone.
    /// Empty groups deliberately mean that no reviewed real-pack art exists.
    pub resources: [Vec<GameFrame>; 4],
    /// Frame-for-frame shadow masks paired with the broadleaf tree group.
    pub tree_shadows: Vec<GameFrame>,
}

impl Renderer {
    pub fn upload_game_atlas(&mut self, pixels: &[u8]) -> Result<(), String> {
        if pixels.len() != GAME_ATLAS_BYTES {
            return Err("Invalid game atlas size".into());
        }
        // Borrowed WASM bytes are synchronously snapshotted by queue.writeTexture;
        // only each page's occupied rows are staged.
        let mut rows = [0; GAME_ATLAS_PAGES as usize];
        for (rows, page) in rows
            .iter_mut()
            .zip(pixels.chunks_exact(GAME_ATLAS_PAGE_BYTES))
        {
            *rows = atlas::occupied_rows(page);
        }
        self.device
            .upload_atlas(pixels, &rows)
            .map_err(crate::web::gpu_bridge::error)?;
        self.atlas_side = GAME_ATLAS_SIDE;
        self.atlas_pages = GAME_ATLAS_PAGES;
        Ok(())
    }

    pub fn render_game(
        &mut self,
        art: &GameArt,
        unit: [f32; 2],
        target: [f32; 2],
        moving: bool,
        animation: usize,
        facing: (usize, bool),
    ) -> Result<Counters, String> {
        let sprites = game_sprites(art, unit, target, moving, animation, facing);
        self.render_sprites(&sprites)
    }
}

pub(crate) fn game_sprites(
    art: &GameArt,
    unit: [f32; 2],
    target: [f32; 2],
    moving: bool,
    animation: usize,
    facing: (usize, bool),
) -> Vec<Sprite> {
    let mut sprites = Vec::new();
    for row in -1_i32..28 {
        for column in -1_i32..11 {
            let frame =
                art.grass[(row * 7 + column * 13).unsigned_abs() as usize % art.grass.len()];
            push(
                &mut sprites,
                frame,
                [
                    column as f32 * 96.0 + (row % 2) as f32 * 48.0,
                    row as f32 * 24.0,
                ],
                1.0,
                false,
            );
        }
    }
    if moving {
        ring(&mut sprites, target, [0.95, 0.79, 0.3, 1.0], 12.0);
    }
    ring(&mut sprites, unit, [0.85, 0.95, 0.65, 1.0], 22.0);
    let frames = if moving { &art.walking } else { &art.standing };
    let frame = frames[facing.0 * 10 + if moving { animation % 10 } else { 0 }];
    push(&mut sprites, frame, unit, 1.35, facing.1);
    sprites
}

fn push(
    sprites: &mut Vec<Sprite>,
    frame: GameFrame,
    position: [f32; 2],
    scale: f32,
    flipped: bool,
) {
    let [w, h] = frame.size.map(|n| n * scale);
    let [ax, ay] = frame.anchor.map(|n| n * scale);
    let x = position[0] - if flipped { w - ax } else { ax };
    let y = position[1] - ay;
    let mut uv = frame.atlas.uv;
    if flipped {
        uv[0] += uv[2];
        uv[2] = -uv[2];
    }
    sprites.push(Sprite {
        position: [(x + w / 2.0) / 480.0 - 1.0, 1.0 - (y + h / 2.0) / 320.0],
        radius: [w / 960.0, h / 640.0],
        color: [1.0; 4],
        uv,
        depths: [0.0; 4],
        terrain_blend: [[0.0; 4]; 2],
        pages: [frame.atlas.page, 0, 0, 0],
    });
}

fn ring(sprites: &mut Vec<Sprite>, p: [f32; 2], color: [f32; 4], radius: f32) {
    for [cos, sin] in RING_POINTS {
        let x = p[0] + cos * radius;
        let y = p[1] + sin * radius * 0.45;
        sprites.push(Sprite {
            position: [x / 480.0 - 1.0, 1.0 - y / 320.0],
            radius: [1.2 / 480.0, 1.2 / 320.0],
            color,
            uv: [
                0.0,
                0.0,
                1.0 / GAME_ATLAS_SIDE as f32,
                1.0 / GAME_ATLAS_SIDE as f32,
            ],
            depths: [0.0; 4],
            terrain_blend: [[0.0; 4]; 2],
            pages: [AtlasAddress::WHITE.page, 0, 0, 0],
        });
    }
}

// Pinned Rust 1.93.1 wasm32 sin/cos at the fixed ring angles.
const RING_POINTS: [[f32; 2]; 48] = [
    [1.0, 0.0],
    [0.9914448857307434, 0.13052619993686676],
    [0.9659258127212524, 0.258819043636322],
    [0.9238795042037964, 0.3826834559440613],
    [0.8660253882408142, 0.5],
    [0.7933533191680908, 0.6087614893913269],
    [0.7071067690849304, 0.7071067690849304],
    [0.6087613701820374, 0.7933533787727356],
    [0.4999999701976776, 0.866025447845459],
    [0.3826834261417389, 0.9238795042037964],
    [0.25881895422935486, 0.9659258723258972],
    [0.1305261254310608, 0.9914448857307434],
    [-4.371138828673793e-8, 1.0],
    [-0.13052621483802795, 0.9914448857307434],
    [-0.2588191628456116, 0.9659258127212524],
    [-0.3826833963394165, 0.9238795638084412],
    [-0.5000000596046448, 0.8660253882408142],
    [-0.6087614297866821, 0.7933533191680908],
    [-0.7071067690849304, 0.7071067690849304],
    [-0.7933533191680908, 0.6087614297866821],
    [-0.8660255074501038, 0.4999998211860657],
    [-0.9238796234130859, 0.38268327713012695],
    [-0.9659258723258972, 0.25881892442703247],
    [-0.9914448261260986, 0.1305263191461563],
    [-1.0, -8.742277657347586e-8],
    [-0.9914448261260986, -0.13052625954151154],
    [-0.9659258127212524, -0.2588190734386444],
    [-0.9238794445991516, -0.382683664560318],
    [-0.8660252690315247, -0.5000001788139343],
    [-0.7933533787727356, -0.6087613701820374],
    [-0.7071068286895752, -0.7071067094802856],
    [-0.6087614893913269, -0.793353259563446],
    [-0.49999991059303284, -0.866025447845459],
    [-0.382683128118515, -0.9238796830177307],
    [-0.25881898403167725, -0.9659258723258972],
    [-0.13052591681480408, -0.9914448857307434],
    [1.1924880638503055e-8, -1.0],
    [0.13052640855312347, -0.9914448261260986],
    [0.25881901383399963, -0.9659258127212524],
    [0.3826836049556732, -0.9238794445991516],
    [0.5000003576278687, -0.8660252094268799],
    [0.6087615489959717, -0.793353259563446],
    [0.7071070075035095, -0.7071065306663513],
    [0.7933533787727356, -0.6087613701820374],
    [0.8660255670547485, -0.4999997615814209],
    [0.9238795638084412, -0.3826834261417389],
    [0.9659257531166077, -0.2588192820549011],
    [0.9914449453353882, -0.13052575290203094],
];

#[cfg(test)]
#[wasm_bindgen_test::wasm_bindgen_test]
fn fixed_ring_points_match_original_trigonometry_bits() {
    for (index, [cos, sin]) in RING_POINTS.into_iter().enumerate() {
        let angle = index as f32 * std::f32::consts::TAU / 48.0;
        assert_eq!(cos.to_bits(), angle.cos().to_bits());
        assert_eq!(sin.to_bits(), angle.sin().to_bits());
    }
}
