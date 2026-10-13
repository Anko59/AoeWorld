use super::*;
use crate::terrain::surface::SurfaceKind;

#[test]
fn provider_cliff_cells_spare_channels_fords_and_textured_banks() {
    // Ratio one: a 240 cm rise across roughly eight two-metre tiles is about fifteen percent.
    let generator = crossing_with_height(
        false,
        Some(&|x, _| (32 - x) * 240),
        vec![[130 * 256, -256, 130 * 256, 258 * 256, 6 * 256, 0, 259 * 256]],
    );
    let tiles = composed(&generator);
    let (mut rivers, mut fords, mut banks, mut cliffs) = (0, 0, 0, 0);
    for point in &tiles {
        let position = point.tile;
        if point.terrain.surface.kind == SurfaceKind::Cliff {
            cliffs += 1;
        }
        if point.terrain.water == WaterKind::River {
            rivers += 1;
            fords += usize::from(point.terrain.material == GroundMaterial::Ford);
            assert_ne!(
                point.terrain.surface.kind,
                SurfaceKind::Cliff,
                "channel {position:?}"
            );
            assert!(
                generator
                    .cliff_piece_with_cancel(position, &|| false)
                    .unwrap()
                    .is_none()
            );
        } else if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
            at(&tiles, position.x + dx, position.y + dy)
                .is_some_and(|neighbor| neighbor.terrain.water == WaterKind::River)
        }) {
            banks += 1;
            assert!(point.terrain.passable, "bank {position:?}");
            assert_ne!(
                point.terrain.surface.kind,
                SurfaceKind::Cliff,
                "bank {position:?}"
            );
            assert_eq!(
                point.terrain.material,
                GroundMaterial::Dirt,
                "bank {position:?}"
            );
            assert!(
                generator
                    .cliff_piece_with_cancel(position, &|| false)
                    .unwrap()
                    .is_none()
            );
        } else {
            continue;
        }
        assert_eq!(
            generator
                .landscape_point_with_cancel(position, &|| false)
                .unwrap()
                .unwrap()
                .tile,
            *point
        );
    }
    assert!(
        rivers > 1000 && fords > 50 && banks > 400 && cliffs > 50,
        "rivers={rivers}, fords={fords}, banks={banks}, cliffs={cliffs}"
    );
}

#[test]
fn dense_cliff_cells_keep_fallback_river_banks_open_and_textured() {
    let mut generator = super::super::mountains::relief(1, 9, |x, _| 1500 - (x - 8) * 432);
    generator.water = None;
    let (mut rivers, mut banks, mut cliffs) = (0, 0, 0);
    for cy in 12..24 {
        for cx in 10..23 {
            let chunk = generator
                .landscape_chunk_with_cancel(cx, cy, &|| false)
                .unwrap();
            for point in &chunk.tiles {
                let position = point.tile;
                cliffs += usize::from(point.terrain.surface.kind == SurfaceKind::Cliff);
                if point.terrain.water == WaterKind::River {
                    rivers += 1;
                    assert_ne!(point.terrain.surface.kind, SurfaceKind::Cliff);
                    assert!(
                        generator
                            .cliff_piece_with_cancel(position, &|| false)
                            .unwrap()
                            .is_none()
                    );
                } else if point.terrain.water == WaterKind::None
                    && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                        generator
                            .base_water_with_cancel(
                                TileCoord::new(position.x + dx, position.y + dy),
                                &|| false,
                            )
                            .unwrap()
                            == Some(WaterKind::River)
                    })
                {
                    banks += 1;
                    assert!(point.terrain.passable, "bank {position:?}");
                    assert_eq!(
                        point.terrain.material,
                        GroundMaterial::Dirt,
                        "bank {position:?}"
                    );
                    assert_ne!(point.terrain.surface.kind, SurfaceKind::Cliff);
                    assert!(
                        generator
                            .cliff_piece_with_cancel(position, &|| false)
                            .unwrap()
                            .is_none()
                    );
                    assert_eq!(
                        generator
                            .landscape_point_with_cancel(position, &|| false)
                            .unwrap()
                            .unwrap()
                            .tile,
                        *point
                    );
                }
            }
        }
    }
    assert!(
        rivers > 100 && banks > 100 && cliffs > 100,
        "rivers={rivers}, banks={banks}, cliffs={cliffs}"
    );
}
