//! Synthetic pixel-pressure regression; no original images or hardware claim.
use super::*;

#[test]
fn castle_ambient_layers_share_object_pages_with_the_full_actor_selection() {
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
            construction: Vec::new(),
            damage: Vec::new(),
        });
    }
    // Full native loops would need 66 large rectangles and overflow the three
    // object pages. Two poses per layer leave room for the current actor set.
    assert_eq!(
        prove(&manifest).unwrap(),
        (1474, 897 + actor_frames() - 115 + 8)
    );
}
