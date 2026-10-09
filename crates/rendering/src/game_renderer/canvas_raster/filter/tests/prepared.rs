//! Frozen scalar/filter oracle: no production sampler or shared texel helper.
use super::*;

fn nearest(atlas: &[u8], address: crate::AtlasAddress, local: [f64; 2]) -> [u8; 4] {
    let rect = address.uv;
    let side = f64::from(GAME_ATLAS_SIDE);
    let x =
        (f64::from(rect[0]) * side + 0.5) + local[0] * (f64::from(rect[2]) * side - 1.0).max(0.0);
    let y =
        (f64::from(rect[1]) * side + 0.5) + local[1] * (f64::from(rect[3]) * side - 1.0).max(0.0);
    let side = GAME_ATLAS_SIDE as usize;
    let x = (x as usize).min(side - 1);
    let y = (y as usize).min(side - 1);
    let Some(start) = (address.page as usize)
        .checked_mul(crate::GAME_ATLAS_PAGE_BYTES)
        .and_then(|base| base.checked_add((y * side + x) * 4))
        .filter(|start| *start <= atlas.len().saturating_sub(4))
    else {
        return [0; 4];
    };
    [
        atlas[start],
        atlas[start + 1],
        atlas[start + 2],
        atlas[start + 3],
    ]
}

fn filtered(
    atlas: &[u8],
    address: crate::AtlasAddress,
    local: [f64; 2],
    kernel: Option<[[f64; 2]; 4]>,
) -> [u8; 4] {
    let center = nearest(atlas, address, local);
    let Some(kernel) = kernel else {
        return center;
    };
    if local.iter().any(|v| !v.is_finite()) {
        return center;
    }
    let mut sums = [0_u32; 4];
    for offset in kernel {
        let uv = std::array::from_fn(|axis| (local[axis] + offset[axis]).clamp(0.0, 1.0));
        let tap = nearest(atlas, address, uv);
        for channel in 0..3 {
            sums[channel] += u32::from(tap[channel]) * u32::from(tap[3]);
        }
        sums[3] += u32::from(tap[3]);
    }
    if sums[3] == 0 {
        return center;
    }
    let mut result = center;
    for channel in 0..3 {
        result[channel] = ((sums[channel] + sums[3] / 2) / sums[3]) as u8;
    }
    result
}

pub(super) fn assert_equivalence(mut atlas: Vec<u8>) {
    // Reuse the existing test's allocation; vary each page, coordinate and alpha.
    for (index, pixel) in atlas.chunks_exact_mut(4).enumerate() {
        let x = index % 2048;
        let y = index / 2048 % 2048;
        let page = index / (2048 * 2048);
        pixel.copy_from_slice(&[
            (x * 17 + page * 53) as u8,
            (y * 29 + page * 71) as u8,
            (x ^ y ^ (page * 97)) as u8,
            [0, 127, 255, 63][(x + y + page) % 4],
        ]);
    }
    let coordinates = [
        f64::NEG_INFINITY,
        -2049.0,
        -1.0,
        -0.0,
        0.0,
        0.5_f64.next_down(),
        0.5,
        0.5_f64.next_up(),
        1.0_f64.next_down(),
        1.0,
        1.0_f64.next_up(),
        2048.0,
        f64::INFINITY,
        f64::NAN,
    ];
    let rects = [
        [0.0, 0.0, 97.0 / 2048.0, 49.0 / 2048.0],
        [1.0 / 2048.0, 2047.0 / 2048.0, 19.0 / 2048.0, 2.0 / 2048.0],
        [-1.0, -0.0, 0.0, -1.0],
        [1.0, 1.0, 1.0, 1.0],
        [f32::NAN, f32::INFINITY, f32::NAN, f32::NEG_INFINITY],
        [f32::NEG_INFINITY, f32::NAN, f32::INFINITY, f32::NAN],
        [0.5_f32.next_up(), 0.5_f32.next_down(), 0.0, 1.0 / 2048.0],
    ];
    let kernels = [
        None,
        Some([[-0.25, -0.25], [-0.25, 0.25], [0.25, -0.25], [0.25, 0.25]]),
    ];
    for page in [0, 1, 2, 3, u32::MAX] {
        for uv in rects {
            let address = crate::AtlasAddress { page, uv };
            let sampler = TerrainSampler::new(&atlas, address);
            for x in coordinates {
                for y in coordinates {
                    for kernel in kernels {
                        assert_eq!(
                            sample_prepared(&atlas, &sampler, [x, y], kernel),
                            filtered(&atlas, address, [x, y], kernel),
                            "page={page} rect={uv:?} local={:?}",
                            [x, y]
                        );
                    }
                }
            }
            // Boundary ULPs exercise origin-plus-product association, not just clamps.
            for axis in 0..2 {
                let extent = (f64::from(uv[axis + 2]) * 2048.0 - 1.0).max(0.0);
                let origin = f64::from(uv[axis]) * 2048.0 + 0.5;
                for target in [0.0, 1.0, 17.0, 2047.0, 2048.0] {
                    let local = (target - origin) / extent;
                    for value in [local.next_down(), local, local.next_up()] {
                        let mut point = [0.5; 2];
                        point[axis] = value;
                        assert_eq!(
                            sampler.sample(&atlas, point),
                            nearest(&atlas, address, point)
                        );
                    }
                }
            }
        }
    }
    // Original valid-prefix behavior remains even for a partial last page.
    for len in [4, crate::GAME_ATLAS_PAGE_BYTES + 4, atlas.len() - 4] {
        let pixels = &atlas[..len];
        for page in [0, 1, 2, 3, u32::MAX] {
            let address = crate::AtlasAddress {
                page,
                uv: [0.0, 0.0, 1.0, 1.0],
            };
            let sampler = TerrainSampler::new(pixels, address);
            for local in [[0.0; 2], [1.0; 2], [f64::NAN, f64::INFINITY]] {
                assert_eq!(
                    sampler.sample(pixels, local),
                    nearest(pixels, address, local)
                );
            }
        }
    }
    // Summed-alpha-zero fallback retains the original nearest center bytes.
    atlas.fill(0);
    let center = 2 * crate::GAME_ATLAS_PAGE_BYTES + (1024 * 2048 + 1024) * 4;
    atlas[center..center + 4].copy_from_slice(&[203, 179, 151, 0]);
    let address = crate::AtlasAddress {
        page: 2,
        uv: [0.0, 0.0, 1.0, 1.0],
    };
    let sampler = TerrainSampler::new(&atlas, address);
    assert_eq!(
        sample_prepared(&atlas, &sampler, [0.5; 2], kernels[1]),
        filtered(&atlas, address, [0.5; 2], kernels[1])
    );
}
