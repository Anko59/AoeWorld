//! Lossless junction observations in the unused third layer rectangle.
//! Four material nibbles per vertex; 15 means no incident land observation.
use super::*;

pub(crate) const FULL: u32 = 1 << 30;
const EMPTY: u8 = 15;

// Water-side 5/11 and forest-bed 6 never occur as land observations. Reuse
// those nibbles for scene materials above 14, preserving empty 15.
fn code(material: u8) -> u8 {
    match material {
        15 => 5,
        16 => 6,
        17 => 11,
        other => other.min(EMPTY),
    }
}

pub(crate) fn material(code: u8) -> u8 {
    match code {
        5 => 15,
        6 => 16,
        11 => 17,
        other => other,
    }
}

pub(super) fn retain(triangle: &mut ProjectedSurfaceTriangle, cells: [[(u8, u16); 4]; 3]) {
    let packed = cells.map(|cells| {
        cells.iter().enumerate().fold(0_u16, |word, (slot, cell)| {
            word | u16::from(code(cell.0)) << (slot * 4)
        })
    });
    let address = AtlasAddress {
        page: 0,
        uv: [
            f32::from(packed[0]),
            f32::from(packed[1]),
            f32::from(packed[2]),
            0.0,
        ],
    };
    triangle.appearance |= FULL;
    triangle.texture_blend = Some([address; 2]);
}

pub(crate) fn packed(triangle: &ProjectedSurfaceTriangle) -> Option<[u16; 3]> {
    (triangle.appearance & FULL != 0)
        .then(|| {
            triangle
                .texture_blend
                .map(|addresses| [0, 1, 2].map(|i| addresses[1].uv[i] as u16))
        })
        .flatten()
}

/// A face's nonzero sheets, in the one global priority order. Missing sheets
/// have zero cumulative weight, so another face can omit them along an edge.
pub(crate) fn palette(packed: [u16; 3]) -> [u8; 12] {
    let mut result = [EMPTY; 12];
    let mut count = 0;
    for word in packed {
        for slot in 0..4 {
            let material = ((word >> (slot * 4)) & 15) as u8;
            if material != EMPTY && !result.contains(&material) {
                result[count] = material;
                count += 1;
            }
        }
    }
    result[..count].sort_unstable_by_key(|code| (rank(material(*code)), *code));
    result
}

/// Six nibbles fit exactly in one f32 integer; two spare packet words hold
/// every possible land sheet, without adding a varying or texture binding.
pub(crate) fn palette_words(palette: [u8; 12]) -> [f32; 2] {
    [0, 1].map(|half| {
        (0..6).fold(0_u32, |word, slot| {
            word | u32::from(palette[half * 6 + slot]) << (slot * 4)
        }) as f32
    })
}

fn share(word: u16, material: u8) -> f32 {
    let (mut present, mut matching) = (0_u32, 0_u32);
    for slot in 0..4 {
        let cell = ((word >> (slot * 4)) & 15) as u8;
        present += u32::from(cell != EMPTY);
        matching += u32::from(cell == material);
    }
    ((matching * 255 + present / 2)
        .checked_div(present)
        .unwrap_or(0)) as f32
        / 255.0
}

pub(crate) fn weights(
    packed: [u16; 3],
    palette: [u8; 12],
    barycentric: [f64; 3],
    noise: f32,
) -> [f32; 15] {
    let mut result = [0.0; 15];
    let water = (0..3)
        .map(|i| (share(packed[i], 10) + share(packed[i], 12)) * barycentric[i] as f32)
        .sum::<f32>();
    let (mut cumulative, mut previous) = (0.0, 0.0);
    for material in palette.into_iter().rev().filter(|m| *m != EMPTY) {
        cumulative += (0..3)
            .map(|i| share(packed[i], material) * barycentric[i] as f32)
            .sum::<f32>();
        let threshold = threshold(cumulative, water, noise);
        result[usize::from(material)] = threshold - previous;
        previous = threshold;
    }
    result
}
