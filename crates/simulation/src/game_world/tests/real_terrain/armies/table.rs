use super::march::run_cell;
use super::scene::{Cell, Shape};
use super::*;
use std::io::Write as _;

/// The table runs only on demand: `.cache/armies/enabled` names the output
/// label. Without it, these tests return at once.
fn cell(shape: Shape, kind: UnitKind, count: usize, general: bool) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.cache/armies");
    let Ok(label) = std::fs::read_to_string(format!("{root}/enabled")) else {
        return;
    };
    let row = run_cell(
        Cell {
            shape,
            kind,
            count,
            general,
        },
        900,
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{root}/{}.tsv", label.trim()))
        .unwrap();
    writeln!(file, "{}", row.tsv()).unwrap();
    let mut diag = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{root}/{}.diag", label.trim()))
        .unwrap();
    writeln!(diag, "== {}\n{}", row.label, row.diagnosis).unwrap();
}

macro_rules! cells {
    ($($name:ident: $shape:ident $kind:ident $count:literal $general:literal;)*) => {
        $(#[test]
        fn $name() {
            cell(Shape::$shape, UnitKind::$kind, $count, $general);
        })*
    };
}

cells! {
    river_infantry_100: River Champion 100 false;
    river_infantry_100_g: River Champion 100 true;
    river_infantry_400: River Champion 400 false;
    river_infantry_400_g: River Champion 400 true;
    river_cavalry_100: River Paladin 100 false;
    river_cavalry_100_g: River Paladin 100 true;
    river_cavalry_400: River Paladin 400 false;
    river_cavalry_400_g: River Paladin 400 true;
    river_ram_100: River SiegeRam 100 false;
    river_ram_100_g: River SiegeRam 100 true;
    river_ram_400: River SiegeRam 400 false;
    river_ram_400_g: River SiegeRam 400 true;
    forest_infantry_100: Forest Champion 100 false;
    forest_infantry_100_g: Forest Champion 100 true;
    forest_infantry_400: Forest Champion 400 false;
    forest_infantry_400_g: Forest Champion 400 true;
    forest_cavalry_100: Forest Paladin 100 false;
    forest_cavalry_100_g: Forest Paladin 100 true;
    forest_cavalry_400: Forest Paladin 400 false;
    forest_cavalry_400_g: Forest Paladin 400 true;
    forest_ram_100: Forest SiegeRam 100 false;
    forest_ram_100_g: Forest SiegeRam 100 true;
    forest_ram_400: Forest SiegeRam 400 false;
    forest_ram_400_g: Forest SiegeRam 400 true;
    pass_infantry_100: Pass Champion 100 false;
    pass_infantry_100_g: Pass Champion 100 true;
    pass_infantry_400: Pass Champion 400 false;
    pass_infantry_400_g: Pass Champion 400 true;
    pass_cavalry_100: Pass Paladin 100 false;
    pass_cavalry_100_g: Pass Paladin 100 true;
    pass_cavalry_400: Pass Paladin 400 false;
    pass_cavalry_400_g: Pass Paladin 400 true;
    pass_ram_100: Pass SiegeRam 100 false;
    pass_ram_100_g: Pass SiegeRam 100 true;
    pass_ram_400: Pass SiegeRam 400 false;
    pass_ram_400_g: Pass SiegeRam 400 true;
    river_mixed_100: River General 100 false;
    river_mixed_100_g: River General 100 true;
    river_mixed_400: River General 400 false;
    river_mixed_400_g: River General 400 true;
    forest_mixed_100_g: Forest General 100 true;
    forest_mixed_400_g: Forest General 400 true;
    pass_mixed_100_g: Pass General 100 true;
    pass_mixed_400_g: Pass General 400 true;
    pass_mangonel_20: Pass Mangonel 20 false;
    pass_mangonel_20_g: Pass Mangonel 20 true;
    river_mangonel_20: River Mangonel 20 false;
    forest_mangonel_20: Forest Mangonel 20 false;
    bank_cavalry_100: Bank Paladin 100 false;
    bank_cavalry_100_g: Bank Paladin 100 true;
    bank_mixed_100_g: Bank General 100 true;
    bank_infantry_100_g: Bank Champion 100 true;
}
