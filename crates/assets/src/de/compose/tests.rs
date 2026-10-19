use super::*;

#[test]
fn every_nonzero_player_mask_preserves_its_blend_weight_and_shade() {
    let masks = [0, 1, 64, 119, 127, 128, 255, 119];
    let frame = Frame {
        width: 12,
        height: 8,
        anchor_x: 2,
        anchor_y: 3,
        index: 0,
        main: Some(Layer {
            x: 4,
            y: 4,
            width: 8,
            height: 1,
            pixels: masks
                .iter()
                .enumerate()
                .map(|(i, _)| [117, 120, 115, if i == 7 { 0 } else { 255 }])
                .collect(),
        }),
        player: Some(Layer {
            x: 4,
            y: 4,
            width: 8,
            height: 1,
            pixels: masks.map(|m| [m, 0, 0, 255]).to_vec(),
        }),
        shadow: None,
    };
    let actual = planes(&frame);
    assert_eq!(
        (
            actual.width,
            actual.height,
            actual.hotspot_x,
            actual.hotspot_y
        ),
        (8, 1, -2, -1)
    );
    for (i, mask) in masks.into_iter().enumerate() {
        let expected = if mask == 0 || i == 7 {
            [0; 4]
        } else {
            [3, 0, 0, mask]
        };
        assert_eq!(
            actual.player[i * 4..i * 4 + 4],
            expected,
            "mask {mask}, pixel {i}"
        );
    }
}

#[test]
fn real_house_preserves_all_nonzero_player_pixels_including_mask_119() {
    let Some(root) = std::env::var_os("AOE_DE_DIR") else {
        return;
    };
    let path =
        std::path::Path::new(&root).join("resources/_common/drs/graphics/b_dark_house_age1_x1.sld");
    let frames = crate::sld::parse(&std::fs::read(path).unwrap()).unwrap();
    let frame = &frames[0];
    let main = frame.main.as_ref().unwrap();
    let mask = frame.player.as_ref().unwrap();
    let at = (100 - usize::from(main.y)) * usize::from(main.width) + 181 - usize::from(main.x);
    assert_eq!(mask.pixels[at][0], 119);
    assert_eq!(main.pixels[at][3], 255);
    let actual = planes(frame);
    let (x0, y0, _, _) = bounds(frame).unwrap();
    let mut marked = 0;
    let mut low = 0;
    for (i, (base, weight)) in main.pixels.iter().zip(&mask.pixels).enumerate() {
        if base[3] == 0 || weight[0] == 0 {
            continue;
        }
        let x = usize::from(main.x - x0) + i % usize::from(main.width);
        let y = usize::from(main.y - y0) + i / usize::from(main.width);
        let pixel = &actual.player[(y * usize::from(actual.width) + x) * 4..][..4];
        assert_eq!(
            pixel[3],
            weight[0],
            "canvas {},{}",
            x + usize::from(x0),
            y + usize::from(y0)
        );
        assert!(pixel[0] <= 7);
        marked += 1;
        low += usize::from(weight[0] < 128);
    }
    assert_eq!((marked, low), (1486, 458));
    eprintln!("real house: {marked} player pixels, {low} below 128; mask 119 retained");
}
