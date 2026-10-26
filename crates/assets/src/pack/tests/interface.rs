use super::*;

#[test]
fn interface_pages_composite_team_colour_and_shadow_without_losing_alpha() {
    assert_eq!(compose_pixel([0; 4], [0, 0, 0, 255], 0), [42, 94, 159, 255]);
    assert_eq!(
        compose_pixel([0; 4], [7, 0, 0, 255], 128),
        [65, 145, 245, 255]
    );
    assert_eq!(
        compose_pixel([120, 80, 40, 255], [0; 4], 128),
        [120, 80, 40, 255]
    );
    assert_eq!(compose_pixel([0; 4], [0; 4], 128), [0, 0, 0, 128]);
    assert_eq!(compose_pixel([99, 88, 77, 0], [0; 4], 0), [0; 4]);
}
