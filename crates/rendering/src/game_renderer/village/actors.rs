use super::*;

pub(super) fn sprite_direction(facing: u8) -> (usize, bool) {
    let facing = facing % 8;
    let row = [0_usize, 1, 2, 3, 4, 3, 2, 1][usize::from(facing)];
    (row, matches!(facing, 1..=3))
}

pub(super) fn unit_direction(unit: SceneUnit) -> (usize, bool) {
    if unit.kind == SceneUnitKind::Stone {
        return (0, false);
    }
    if unit.kind.native_directions() {
        // DE starts east, clockwise; scene headings start south through east.
        return (usize::from(unit.facing & 15), false);
    }
    sprite_direction(unit.facing)
}

pub(super) fn unit_frame_index(unit: SceneUnit, stride: usize, phase: usize) -> (usize, bool) {
    let (direction, mirrored) = unit_direction(unit);
    (direction * stride + phase % stride.max(1), mirrored)
}

pub fn actor_frame_index(facing: u8, stride: usize, phase: usize) -> (usize, bool) {
    let (direction, mirrored) = sprite_direction(facing);
    (direction * stride + phase % stride.max(1), mirrored)
}
