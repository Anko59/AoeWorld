use super::*;

#[test]
fn the_first_eight_players_have_the_aoe_palette_and_a_hundred_players_are_distinct() {
    let blue = PLAYER_COLOURS[0];
    for (id, colour) in PLAYER_COLOURS.into_iter().enumerate() {
        assert_eq!(
            player_colour(PlayerId(id as u16), PlayerId(0), blue),
            colour
        );
    }
    let colours = (0..100)
        .map(|id| player_colour(PlayerId(id), PlayerId(0), blue))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(colours.len(), 100);
    assert_ne!(
        player_colour(PlayerId(0), PlayerId(0), blue),
        player_colour(PlayerId(8), PlayerId(0), blue)
    );
    assert_eq!(
        player_colour(PlayerId(99), PlayerId(0), blue),
        [249, 235, 130]
    );
}

#[test]
fn a_banner_choice_swaps_palette_slots_without_disguising_an_enemy_as_the_local_player() {
    for local in 0..8 {
        for chosen in PLAYER_COLOURS {
            assert_eq!(
                player_colour(PlayerId(local), PlayerId(local), chosen),
                chosen
            );
            let colours = (0..8)
                .map(|id| player_colour(PlayerId(id), PlayerId(local), chosen))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(colours.len(), 8);
        }
    }
}

#[test]
fn player_mask_keeps_unmasked_blue_and_composites_partial_masks_with_native_alpha() {
    assert_eq!(
        player_texel([65, 145, 245, 255], [0; 4], [245, 65, 65]),
        [65, 145, 245, 255]
    );
    assert_eq!(
        player_texel([65, 145, 245, 255], [7, 255, 0, 0], [245, 65, 65]),
        [245, 65, 65, 255]
    );
    assert_eq!(
        player_texel([117, 120, 115, 187], [7, 119, 128, 0], [65, 145, 245]),
        [84, 136, 198, 187]
    );
    assert_eq!(
        player_texel([0, 0, 0, 255], [0, 255, 0, 0], [245, 65, 65]),
        [159, 42, 42, 255]
    );
}

#[test]
fn ownership_visual_keeps_motion_and_colours_dead_bodies_without_a_hidden_contour() {
    let living = SceneUnitVisual::new(true).with_owner(Some([245, 65, 65]), true, true);
    assert!(living.moving() && living.hostile());
    assert!(living.with_selected(true).selected());
    assert!(!living.with_selected(true).with_selected(false).selected());
    assert_eq!(living.outline_colour(), Some([245, 65, 65]));
    let mut dead = living.with_owner(Some([245, 65, 65]), true, false);
    dead.set_moving(false);
    assert!(!dead.moving());
    assert_eq!(dead.colour(), Some([245, 65, 65]));
    assert_eq!(dead.outline_colour(), None);
    assert_eq!(std::mem::size_of::<SceneUnitVisual>(), 2);
}

#[test]
fn compact_visual_roundtrips_all_supported_players_and_preserves_selection() {
    for id in 0..100 {
        let player = PlayerId(id);
        let visual = SceneUnitVisual::new(true).with_selected(true).with_player(
            Some(player),
            PlayerId(0),
            PLAYER_COLOURS[0],
            true,
        );
        assert_eq!(
            visual.colour(),
            Some(player_colour(player, PlayerId(0), PLAYER_COLOURS[0]))
        );
        assert_eq!(visual.hostile(), id != 0);
        assert_eq!(visual.outline_colour(), visual.colour());
        assert!(visual.selected() && visual.moving());
    }
}
