//! Synthetic pixel-pressure regression; no original images or hardware claim.
use super::*;

#[test]
fn castle_full_timelines_share_bounded_native_residency_with_the_full_actor_selection() {
    let mut manifest = synthetic_manifest();
    for (unit_id, count) in [(130, 10), (18, 23)] {
        let mut parts = Vec::new();
        for layer in 0..2 {
            let source = format!("fixture:castle:{unit_id}:{layer}");
            for frame in 0..count {
                let mut record = manifest.frames[0].clone();
                record.source = source.clone();
                record.frame = frame;
                record.width = 1000;
                record.height = 600;
                manifest.frames.push(record);
            }
            parts.push(crate::dat::BuildingPart {
                graphic_id: unit_id,
                slp_id: unit_id,
                source,
                available: true,
                frames: count as u16,
                angles: 1,
                primary: layer == 0,
                replay_delay: 0.0,
                frame_rate: 0.2,
                offset: [0; 2],
                misplacement: [0.0; 2],
                layer,
            });
        }
        manifest.buildings.push(crate::dat::BuildingComposition {
            unit_id,
            footprint: [3.0; 2],
            foundation_terrain: 0,
            foundation_slp: None,
            foundation_size: [0; 2],
            parts,
            states: Vec::new(),
            construction: Vec::new(),
            damage: Vec::new(),
        });
    }
    let large: Vec<_> = manifest
        .frames
        .iter()
        .filter(|f| f.source.starts_with("fixture:castle:"))
        .map(|f| FrameExtent {
            domain: AtlasDomain::Objects,
            width: f.width,
            height: f.height,
        })
        .collect();
    assert!(
        crate::catalog::packing::pack_frames(&large).is_err(),
        "all large source cells exceed the fixed startup object pages"
    );
    manifest.chunks.push(crate::pack::frame_chunks::FrameChunk {
        file: "synthetic.bin".into(),
        hash: "a".repeat(64),
        bytes: 1,
    });
    for f in manifest
        .frames
        .iter_mut()
        .filter(|f| f.source.starts_with("fixture:castle:"))
    {
        f.native = Some(crate::pack::frame_chunks::FrameLocation {
            chunk: 0,
            offset: 0,
            length: 1,
        });
    }
    assert_eq!(
        prove(&manifest).unwrap(),
        (1474, 897 + actor_frames() - 115 + 66)
    );
    let first = manifest
        .frames
        .iter_mut()
        .find(|f| f.native.is_some())
        .unwrap();
    first.width = 2045;
    assert!(
        prove(&manifest).is_err(),
        "native cells still obey the physical extent bound"
    );
}

#[test]
fn full_catalog_with_native_mill_dock_and_farm_fits_runtime_budget() {
    let mut manifest = synthetic_manifest();
    // Independent metadata fixture: disjoint native Mill body/sails, Dock wake
    // and Farm terrain. Do not manufacture original pixels for this proof.
    for (unit_id, count, terrain) in [(68, 44, false), (45, 60, false), (50, 36, true)] {
        let source = format!("fixture:{unit_id}");
        for frame in 0..count {
            let mut record = manifest.frames[0].clone();
            record.source = source.clone();
            record.frame = frame;
            manifest.frames.push(record);
        }
        manifest.buildings.push(crate::dat::BuildingComposition {
            unit_id,
            footprint: [3.0; 2],
            foundation_terrain: 0,
            foundation_slp: terrain.then_some(unit_id),
            foundation_size: if terrain { [6, 6] } else { [0, 0] },
            parts: if terrain {
                Vec::new()
            } else {
                vec![crate::dat::BuildingPart {
                    graphic_id: unit_id,
                    slp_id: unit_id,
                    source,
                    available: true,
                    frames: count as u16,
                    angles: 1,
                    primary: true,
                    replay_delay: 0.0,
                    frame_rate: 0.15,
                    offset: [0; 2],
                    misplacement: [0.0; 2],
                    layer: 20,
                }]
            },
            states: Vec::new(),
            construction: Vec::new(),
            damage: Vec::new(),
        });
        if terrain {
            for frame in manifest.frames.iter_mut().rev().take(count as usize) {
                frame.source = format!("terrain.drs:[32, 112, 108, 115]:{unit_id}");
            }
        }
    }
    assert_eq!(
        prove(&manifest).unwrap(),
        (1510, 897 + actor_frames() - 115 + 104)
    );
}
