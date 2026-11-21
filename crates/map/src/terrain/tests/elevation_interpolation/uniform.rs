use super::*;

#[test]
fn verified_uniform_elevation_has_no_relief_and_preserves_all_samples() {
    let (mut environment, mut pages, _) = small_plane();
    let height = -12_345;
    for (page, level) in pages.iter_mut().zip(&mut environment.elevation.levels) {
        page.geographic_height_centimeters.fill(height);
        level.ordered_page_root = ordered_page_root(std::slice::from_ref(page)).unwrap();
    }
    let generator = generator(7)
        .with_prepared_elevation(Ratio::new(30, 1).unwrap(), &environment, pages)
        .unwrap();
    let elevation = generator.elevation.as_ref().unwrap();
    assert!(!elevation.has_relief());
    for at in [
        TileCoord::new(0, 0),
        TileCoord::new(2, 2),
        TileCoord::new(4, 4),
    ] {
        assert_eq!(elevation.height_at(at, 5), Some(height));
        assert_eq!(elevation.corner_heights(at, 5), Some([height; 4]));
        assert_eq!(
            generator.cliff_piece_with_cancel(at, &|| false).unwrap(),
            None
        );
    }
}

#[test]
fn a_short_elevation_page_cannot_certify_uniform_coverage() {
    let (mut environment, mut pages, _) = small_plane();
    pages[0].width = 1;
    pages[0].geographic_height_centimeters = vec![100; 2];
    environment.elevation.levels[0].ordered_page_root = ordered_page_root(&pages[..1]).unwrap();
    let generator = generator(7)
        .with_prepared_elevation(Ratio::new(30, 1).unwrap(), &environment, pages)
        .unwrap();
    let elevation = generator.elevation.as_ref().unwrap();
    assert!(elevation.has_relief());
    assert_eq!(elevation.height_at(TileCoord::new(4, 4), 5), None);
}
