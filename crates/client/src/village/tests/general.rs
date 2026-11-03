use super::super::vassal::{panel_command, panel_values};
use aoe_core::EntityId;
use aoe_protocol::{GeneralCommand, GeneralState, GeneralStatus};
use wasm_bindgen_test::wasm_bindgen_test;

fn status() -> GeneralStatus {
    GeneralStatus {
        army_size: 42,
        state: GeneralState::Fighting,
        training: true,
        queued: 3,
        mix: [35, 30, 20, 10, 5],
        counts: [20, 10, 6, 4, 2],
    }
}

#[wasm_bindgen_test]
fn the_panel_receives_state_training_mix_and_counts_in_a_fixed_layout() {
    assert_eq!(
        panel_values(EntityId(9), status()),
        [
            9.0, 42.0, 2.0, 1.0, 3.0, 35.0, 30.0, 20.0, 10.0, 5.0, 20.0, 10.0, 6.0, 4.0, 2.0
        ]
    );
}

#[wasm_bindgen_test]
fn the_train_button_and_each_slider_become_general_commands() {
    let mix = status().mix;
    assert_eq!(
        panel_command(9, 1, mix),
        Some(GeneralCommand::SetTraining { enabled: true })
    );
    assert_eq!(
        panel_command(9, 0, mix),
        Some(GeneralCommand::SetTraining { enabled: false })
    );
    // Role 2 (cavalry) to 70 percent changes only that share.
    assert_eq!(
        panel_command(10, 2 * 128 + 70, mix),
        Some(GeneralCommand::SetMix {
            mix: [35, 30, 70, 10, 5]
        })
    );
    assert_eq!(panel_command(10, 9 * 128, mix), None, "no such role");
}
