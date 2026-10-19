use super::*;

#[test]
fn preview_blends_player_colour_with_the_same_mask_weight_as_the_client() {
    let mut planes = Planes::blank(3, 1, 0, 0);
    planes.color = [117, 120, 115, 255].repeat(3);
    planes.player = vec![7, 0, 0, 0, 7, 0, 0, 119, 7, 0, 0, 255];
    assert_eq!(render(&planes), [117, 120, 115, 93, 132, 176, 65, 145, 245]);
}
