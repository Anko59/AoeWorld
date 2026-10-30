use super::*;
#[test]
fn every_native_texel_survives_cell_splitting_and_lods_are_separate() {
    let width = 512;
    let height = 256;
    let texture = dds::Texture {
        width,
        height,
        pixels: (0..usize::from(width) * usize::from(height))
            .map(|n| [n as u8, (n / 512) as u8, 91, 255])
            .collect(),
    };
    let d = Decoder::new(texture);
    assert_eq!(d.len(), 7);
    for f in 0..2 {
        let p = d.frame(f).unwrap();
        assert_eq!((p.width, p.height), (256, 256));
        for y in 0..256 {
            for x in 0..256 {
                assert_eq!(
                    &p.color[(y * 256 + x) * 4..(y * 256 + x + 1) * 4],
                    &[(x + f * 256) as u8, y as u8, 91, 255]
                );
            }
        }
    }
    assert_eq!(d.frame(2).unwrap().width, 256);
    assert!(d.frame(7).is_err());
}
