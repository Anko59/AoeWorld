//! One landscape appearance fixture rendered by Canvas, WebGL2 and WebGPU.
//! Every backend asserts the same expected texel, so a palette scale, canopy
//! factor or material gate changed in only one kernel fails that backend.
use super::{floor_weights, pack, texel};
use crate::surface_mesh::{ProjectedSurfaceTriangle, procedural_tint};
use crate::{AtlasAddress, SceneTerrainAppearance};

/// Page texels: primary ground, forest-floor blend layer, unused third layer.
/// Bright channels below 245 never saturate under the 1.04 palette scale, so
/// a 1% scale or canopy change at tint 0 moves a channel by more than ±1.
pub(crate) const TEXELS: [[u8; 4]; 3] = [
    [240, 236, 232, 255],
    [232, 240, 228, 255],
    [40, 60, 220, 255],
];

pub(crate) struct Case {
    pub(crate) material: u8,
    pub(crate) tint: u8,
    pub(crate) word: u32,
    pub(crate) blend: bool,
    /// `None`: the packet must be ignored, matching the same face rendered
    /// with appearance zero on that backend.
    pub(crate) expected: Option<[u8; 4]>,
}

pub(crate) fn atlas() -> Vec<u8> {
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    for (page, value) in TEXELS.iter().enumerate() {
        let start = page * crate::GAME_ATLAS_PAGE_BYTES;
        atlas[start..start + 4].copy_from_slice(value);
    }
    atlas
}

fn word(palette: u8, floor_strength: u16, canopy_strength: u16) -> u32 {
    pack(Some(SceneTerrainAppearance {
        floor_strength,
        canopy_strength,
        palette,
        exposure: 1,
        height_band: 2,
    }))
}

pub(crate) fn cases() -> Vec<Case> {
    // (material, tint, palette, floor, canopy, blend): grass 0, dry grass 1,
    // dirt 2 and forest floor 6, every palette, shade tint and canopy extreme.
    let landscape = [
        (0, 0, 0, 0, 0, true),
        (6, 1, 0, 1000, 1000, true),
        (6, 0, 1, 1000, 0, true),
        (0, 2, 1, 400, 500, true),
        (2, 0, 2, 500, 0, true),
        (6, 3, 2, 650, 250, true),
        (1, 0, 3, 250, 0, true),
        (1, 2, 3, 800, 1000, true),
        (2, 0, 4, 750, 0, true),
        (6, 1, 4, 100, 300, false),
        (0, 0, 5, 1000, 1000, true),
        (6, 3, 5, 0, 0, true),
    ];
    let mut cases = landscape
        .into_iter()
        .map(|(material, tint, palette, floor, canopy, blend)| {
            let word = word(palette, floor, canopy);
            let weights = if blend {
                floor_weights(word)
            } else {
                [1.0, 0.0, 0.0]
            };
            let samples = if blend { TEXELS } else { [TEXELS[0]; 3] };
            Case {
                material,
                tint,
                word,
                blend,
                expected: Some(texel(samples, weights, tint, word)),
            }
        })
        .collect::<Vec<_>>();
    // Procedural rock, snow and a rock ramp keep their own kernel.
    for (material, tint) in [(4, 5), (7, 6), (4, 21)] {
        cases.push(Case {
            material,
            tint,
            word: word(3, 900, 1000),
            blend: false,
            expected: Some(procedural_tint(TEXELS[0], tint)),
        });
    }
    // Sand is outside the vegetative gate: legacy barycentric blend.
    cases.push(Case {
        material: 3,
        tint: 0,
        word: word(3, 900, 1000),
        blend: true,
        expected: None,
    });
    cases
}

/// Applies the case to a backend's 16/112 test triangle on pages 0, 1, 2.
pub(crate) fn apply(case: &Case, triangle: &mut ProjectedSurfaceTriangle, legacy: bool) {
    let address = |page| AtlasAddress {
        page,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    triangle.material = case.material;
    triangle.tint = case.tint;
    triangle.skirt = false;
    triangle.appearance = if legacy { 0 } else { case.word };
    triangle.texture_uv = Some(address(0));
    triangle.texture_blend = case.blend.then(|| [address(1), address(2)]);
}
