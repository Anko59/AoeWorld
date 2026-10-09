//! Synthetic native-address fixtures; no real geography or production sampler gold.
use crate::surface_mesh::{
    ProjectedSurfaceTriangle, SurfacePoint, apply_terrain_textures, landscape,
};
use crate::{AtlasAddress, GameArt, GameFrame, SceneTerrainAppearance, TerrainTopology};
use aoe_assets::catalog::{
    TerrainFrameTopology,
    packing::{Placement, terrain_lookup as lut},
};

pub(crate) const LOW: [[u32; 2]; 5] = [[51, 55], [57, 55], [63, 67], [69, 73], [55, 75]];
pub(crate) const BOUNDARY: [[u32; 2]; 5] = [[40, 40], [60, 40], [40, 60], [60, 60], [80, 80]];

pub(crate) struct Fixture {
    pub atlas: Vec<u8>,
    pub art: GameArt,
    placements: [Vec<Placement>; 7],
}

impl Fixture {
    pub fn new() -> Self {
        let mut fixture = Self {
            atlas: vec![0; crate::GAME_ATLAS_BYTES],
            art: GameArt {
                walking: Vec::new(),
                standing: Vec::new(),
                grass: Vec::new(),
                terrain: std::array::from_fn(|_| Vec::new()),
                terrain_topology: [None; 7],
                terrain_world: None,
                resources: std::array::from_fn(|_| Vec::new()),
                tree_shadows: Vec::new(),
                tree_families: Default::default(),
            },
            placements: std::array::from_fn(|_| Vec::new()),
        };
        fixture.layout(false);
        fixture
    }

    /// Change only declared addresses/key, retaining one 48MiB owner and old art pixels.
    pub fn layout(&mut self, shifted: bool) {
        self.atlas[lut::ROW_OFFSET..lut::ROW_OFFSET + lut::ROW_BYTES].fill(0);
        self.placements = std::array::from_fn(|group| {
            let (count, base) = match group {
                0 => (6, 0),
                6 => (10, 6),
                _ => (0, 0),
            };
            (0..count)
                .map(|i| Placement {
                    page: ((base + i) % 2) as u16,
                    x: (2 + 101 * ((base + i) / 2)) as u16,
                    y: if shifted { 60 } else { 2 },
                    width: 97,
                    height: 49,
                })
                .collect()
        });
        let groups = std::array::from_fn(|group| lut::TerrainGroup {
            placements: &self.placements[group],
            topology: match group {
                0 => Some(TerrainFrameTopology::PeriodicXMajorReversedY {
                    columns: 2,
                    rows: 3,
                }),
                6 => Some(TerrainFrameTopology::CoordinateStableAccents),
                _ => None,
            },
        });
        let metadata =
            lut::write_table(&mut self.atlas, &groups, lut::ExistingTable::Reject).unwrap();
        self.art.terrain_world = Some(metadata.layout_checksum);
        self.art.terrain_topology = [None; 7];
        self.art.terrain_topology[0] = Some(TerrainTopology::PeriodicXMajorReversedY {
            columns: 2,
            rows: 3,
        });
        self.art.terrain_topology[6] = Some(TerrainTopology::CoordinateStableAccents);
        self.art.terrain = std::array::from_fn(|group| {
            self.placements[group]
                .iter()
                .map(|p| GameFrame {
                    atlas: AtlasAddress {
                        page: u32::from(p.page),
                        uv: [
                            f32::from(p.x) / 2048.0,
                            f32::from(p.y) / 2048.0,
                            f32::from(p.width) / 2048.0,
                            f32::from(p.height) / 2048.0,
                        ],
                    },
                    size: [97.0, 49.0],
                    anchor: [0.0; 2],
                })
                .collect()
        });
        self.art.grass = self.art.terrain[0].clone();
        self.paint(0);
    }

    /// 0 opaque constants; 1 checker with opaque NW center; 2 same checker center hole.
    pub fn paint(&mut self, pattern: u8) {
        for group in [0, 6] {
            for (index, placement) in self.placements[group].iter().enumerate() {
                for y in 0..49_usize {
                    for x in 0..97_usize {
                        let mut rgba = color(group, index);
                        if pattern != 0 {
                            // Odd-address frames retain checker taps; even frames are
                            // transparent except the explicitly opaque center in pattern1.
                            rgba[3] = if (x + y) % 2 != 0 && index % 2 != 0 {
                                255
                            } else {
                                0
                            };
                            if pattern == 1 && x == 48 && y == 0 {
                                rgba[3] = 255;
                            }
                        }
                        let offset = usize::from(placement.page) * crate::GAME_ATLAS_PAGE_BYTES
                            + ((usize::from(placement.y) + y) * 2048
                                + usize::from(placement.x)
                                + x)
                                * 4;
                        self.atlas[offset..offset + 4].copy_from_slice(&rgba);
                    }
                }
            }
        }
    }

    /// Reconstruct only test layouts through the real strict owned constructor.
    /// Decoder use is fixture-only; shipping admission consumes constructor proof.
    pub fn constructed(&self) -> lut::ConstructedTerrainAtlas {
        let table = lut::validated_table(&self.atlas).unwrap().unwrap();
        let metadata = table.metadata();
        let placements: [Vec<Placement>; 7] = std::array::from_fn(|slot| {
            let group = metadata.groups[slot];
            (0..group.count)
                .map(|index| table.descriptor(group.base + index).unwrap())
                .collect()
        });
        let groups = std::array::from_fn(|slot| lut::TerrainGroup {
            placements: &placements[slot],
            topology: metadata.groups[slot].topology,
        });
        let mut pixels = self.atlas.clone();
        pixels[lut::ROW_OFFSET..lut::ROW_OFFSET + lut::ROW_BYTES].fill(0);
        lut::ConstructedTerrainAtlas::new(pixels, &groups).unwrap()
    }

    pub fn reject_invalid_art(
        &mut self,
        atlas: &lut::ConstructedTerrainAtlas,
        mut upload: impl FnMut(&lut::ConstructedTerrainAtlas, &GameArt) -> Result<(), String>,
    ) {
        let key = self.art.terrain_world;
        self.art.terrain_world = key.map(|key| key ^ 1);
        assert!(upload(atlas, &self.art).is_err());
        self.art.terrain_world = key;
        let original = self.art.terrain[0][0].atlas;
        self.art.terrain[0][0].atlas.uv[0] += 1.0 / 2048.0;
        assert!(upload(atlas, &self.art).is_err());
        self.art.terrain[0][0].atlas = original;
        let frame = self.art.terrain[0].pop().unwrap();
        assert!(upload(atlas, &self.art).is_err());
        self.art.terrain[0].push(frame);
        let topology = self.art.terrain_topology[0];
        self.art.terrain_topology[0] = Some(TerrainTopology::CoordinateStableAccents);
        assert!(upload(atlas, &self.art).is_err());
        self.art.terrain_topology[0] = topology;
    }

    pub fn corrupt(&mut self, zero: bool) {
        if zero {
            self.atlas[lut::ROW_OFFSET..lut::ROW_OFFSET + lut::ROW_BYTES].fill(0);
        } else {
            self.atlas[lut::ROW_OFFSET + 12] ^= 1;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Case {
    pub size: i32,
    pub zoom: bool,
    pub diagonal: bool,
    pub ramp: bool,
    pub floor: u16,
    pub origin: [i32; 2],
}

/// Pairwise matrix, not a claim of the full Cartesian cross product.
pub(crate) fn cases() -> [Case; 12] {
    std::array::from_fn(|i| Case {
        size: [2, 4, 8][i % 3],
        zoom: i % 2 != 0,
        diagonal: i / 2 % 2 != 0,
        ramp: i / 4 % 2 != 0,
        floor: match i {
            9 | 11 => 1000,
            10 => 650,
            _ => [0, 650, 1000][i / 3 % 3],
        },
        origin: match i {
            9 => [i32::MIN, i32::MAX],
            10 => [i32::MAX - 1, i32::MIN + 1],
            11 => [i32::MAX, i32::MAX],
            _ => [-7, 11],
        },
    })
}

pub(crate) fn probes(case: Case) -> [[u32; 2]; 5] {
    if case.zoom {
        LOW
    } else {
        LOW.map(|p| p.map(|v| 3 * v - 127))
    }
}

pub(crate) fn faces(art: &GameArt, case: Case, fine: bool) -> Vec<ProjectedSurfaceTriangle> {
    let (left, side) = if case.zoom {
        (48.0, 32.0)
    } else {
        (16.0, 96.0)
    };
    rectangle(art, case, fine, left, side)
}

pub(crate) fn boundary_faces(art: &GameArt) -> Vec<ProjectedSurfaceTriangle> {
    rectangle(
        art,
        Case {
            size: 4,
            zoom: false,
            diagonal: false,
            ramp: false,
            floor: 0,
            origin: [-7, 11],
        },
        false,
        20.5,
        80.0,
    )
}

fn rectangle(
    art: &GameArt,
    case: Case,
    fine: bool,
    left: f64,
    side: f64,
) -> Vec<ProjectedSurfaceTriangle> {
    let cells = if fine { case.size } else { 1 };
    let step = case.size / cells;
    let mut output = Vec::with_capacity((cells * cells * 2) as usize);
    for y in 0..cells {
        for x in 0..cells {
            let offset = [x * step, y * step];
            let origin = [
                case.origin[0].wrapping_add(offset[0]),
                case.origin[1].wrapping_add(offset[1]),
            ];
            let corners = [[0, 0], [1, 0], [1, 1], [0, 1]].map(|corner| {
                let q = [offset[0] + corner[0] * step, offset[1] + corner[1] * step];
                SurfacePoint {
                    world: [
                        f64::from(origin[0]) + f64::from(corner[0] * step),
                        f64::from(origin[1]) + f64::from(corner[1] * step),
                        if case.ramp {
                            f64::from(q[0] + q[1]) * 0.125
                        } else {
                            0.0
                        },
                    ],
                    screen: aoe_core::ScreenPoint {
                        x: left + f64::from(q[0]) * side / f64::from(case.size),
                        y: left + f64::from(q[1]) * side / f64::from(case.size),
                    },
                }
            });
            let (indices, modes) = if case.diagonal {
                ([[0, 1, 3], [1, 2, 3]], [2, 3])
            } else {
                ([[0, 1, 2], [0, 2, 3]], [0, 1])
            };
            for (indices, mode) in indices.into_iter().zip(modes) {
                output.push(ProjectedSurfaceTriangle {
                    points: indices.map(|i| corners[i]),
                    color: [0.0; 3],
                    tile: origin,
                    skirt: false,
                    material: 0,
                    appearance: landscape::pack(Some(SceneTerrainAppearance {
                        floor_strength: case.floor,
                        canopy_strength: 0,
                        palette: 5,
                        exposure: 0,
                        height_band: 0,
                    })),
                    floor_strengths: Some([((u32::from(case.floor) * 255 + 500) / 1000) as u8; 3]),
                    texture_mode: mode,
                    tint: 0,
                    texture_uv: None,
                    texture_blend: None,
                    texture_tile: origin,
                    texture_materials: None,
                    pickable: true,
                    order: mode,
                });
            }
        }
    }
    apply_terrain_textures(&mut output, art);
    for face in &output {
        let world = face
            .world_texture()
            .expect("explicit actual qualified world provenance");
        assert_eq!(world.groups, [0, 6]);
        assert_eq!(world.footprint, [step as f32; 2]);
        assert_eq!(world.checksum, art.terrain_world.unwrap());
    }
    output
}

pub(crate) fn legacy(faces: &[ProjectedSurfaceTriangle]) -> Vec<ProjectedSurfaceTriangle> {
    faces
        .iter()
        .copied()
        .map(|mut face| {
            if let Some(ref mut materials) = face.texture_materials {
                materials[2] &= 127;
            }
            if let (Some(blend), Some(primary)) = (&mut face.texture_blend, face.texture_uv) {
                blend[1] = primary;
            }
            assert!(face.world_texture().is_none());
            face
        })
        .collect()
}

pub(crate) fn color(group: usize, index: usize) -> [u8; 4] {
    if group == 0 {
        [
            30 + index as u8 * 17,
            90 + index as u8 * 11,
            20 + index as u8 * 9,
            255,
        ]
    } else {
        [
            110 + index as u8 * 7,
            35 + index as u8 * 5,
            15 + index as u8 * 3,
            255,
        ]
    }
}

pub(crate) fn selector(group: usize, owner: [i32; 2]) -> usize {
    if group == 0 {
        (owner[0].rem_euclid(2) * 3 + (-i64::from(owner[1])).rem_euclid(3) as i32) as usize
    } else {
        owner[0]
            .wrapping_mul(7)
            .wrapping_add(owner[1].wrapping_mul(13))
            .unsigned_abs() as usize
            % 10
    }
}

pub(crate) fn expected(case: Case, probe: [u32; 2]) -> [u8; 4] {
    let (left, side) = if case.zoom {
        (48.0, 32.0)
    } else {
        (16.0, 96.0)
    };
    let owner = std::array::from_fn(|a| {
        case.origin[a].wrapping_add(
            ((f64::from(probe[a]) + 0.5 - left) * f64::from(case.size) / side).floor() as i32,
        )
    });
    let a = color(0, selector(0, owner));
    let b = color(6, selector(6, owner));
    let weight = ((u32::from(case.floor) * 255 + 500) / 1000) as f64 / 255.0;
    std::array::from_fn(|c| {
        (f64::from(a[c]) * (1.0 - weight) + f64::from(b[c]) * weight).round() as u8
    })
}

/// q integer center, dx=(.05,0), dy=(0,.05): four different world owners.
/// Native checker taps (48,47),(95,24),(1,24),(48,1) all have odd parity;
/// only odd FRAME indices contribute alpha255. Independent alpha-weighted gold.
pub(crate) fn boundary_expected(probe: [u32; 2]) -> [u8; 4] {
    let cell = probe.map(|v| ((f64::from(v) + 0.5 - 20.5) * 4.0 / 80.0) as i32);
    let mut sum = [0_u32; 3];
    let mut count = 0;
    for [dx, dy] in [[-1, -1], [-1, 0], [0, -1], [0, 0]] {
        let owner = [-7 + cell[0] + dx, 11 + cell[1] + dy];
        let index = selector(0, owner);
        if index % 2 == 0 {
            continue;
        }
        let sample = color(0, index);
        for c in 0..3 {
            sum[c] += u32::from(sample[c]);
        }
        count += 1;
    }
    assert_eq!(
        count, 2,
        "boundary fixture must retain two opaque and two zero-alpha taps"
    );
    [
        ((sum[0] + count / 2) / count) as u8,
        ((sum[1] + count / 2) / count) as u8,
        ((sum[2] + count / 2) / count) as u8,
        255,
    ]
}

pub(crate) fn assert_rgba(actual: [u8; 4], expected: [u8; 4]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!(a.abs_diff(e) <= 1, "{actual:?} != {expected:?}");
    }
}
