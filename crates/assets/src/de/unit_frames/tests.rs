use super::*;
#[test]
fn delta_offsets_crop_union_and_player_weights_follow_draw_order() {
    let mut hull = Planes::blank(2, 1, 1, 0);
    hull.color = vec![1, 2, 3, 255, 4, 5, 6, 255];
    hull.player[0..4].copy_from_slice(&[3, 0, 0, 99]);
    hull.shadow[7] = 80;
    let mut sail = Planes::blank(1, 2, 0, 1);
    sail.color = vec![7, 8, 9, 255, 10, 11, 12, 255];
    sail.player[4..8].copy_from_slice(&[6, 0, 0, 128]);
    let out = merge(&[hull, sail]).unwrap();
    assert_eq!(
        (out.width, out.height, out.hotspot_x, out.hotspot_y),
        (2, 2, 1, 1)
    );
    assert_eq!(&out.color[12..16], &[10, 11, 12, 255]);
    assert_eq!(&out.player[12..16], &[6, 0, 0, 128]);
    assert_eq!(&out.player[8..12], &[3, 0, 0, 99]);
    assert_eq!(out.shadow[15], 80);
}

#[test]
fn the_generic_decoder_preserves_every_native_cell_anchor_and_plane() {
    use crate::sld::{Frame, Layer};
    for count in [481, 721, 960] {
        let frames: Vec<_> = (0..count)
            .map(|index| {
                let layer = Layer {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                    pixels: vec![[index as u8, 2, 3, 255]],
                };
                Frame {
                    width: 32,
                    height: 32,
                    anchor_x: index,
                    anchor_y: 17,
                    index,
                    main: Some(layer.clone()),
                    shadow: Some(layer.clone()),
                    player: Some(layer),
                }
            })
            .collect();
        let expected: Vec<_> = frames.iter().map(compose::planes).collect();
        let mut decoder = Decoder {
            source: None,
            parts: vec![frames],
            stream: None,
        };
        assert_eq!(decoder.len(), usize::from(count));
        for (i, expected) in expected.iter().enumerate() {
            let p = decoder.frame(i).unwrap();
            assert_eq!((p.hotspot_x, p.hotspot_y), (i as i32, 17));
            assert_eq!(p.color, expected.color);
            assert_eq!(p.player, expected.player);
            assert_eq!(p.shadow, expected.shadow);
        }
        assert!(decoder.frame(usize::from(count)).is_err());
    }
}
