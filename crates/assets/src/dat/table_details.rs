//! Local composition evidence shared by the JSON table and its Markdown view.
use super::{DatFile, table::Table};
use std::{collections::BTreeSet, fmt::Write};

pub(super) fn compositions(
    file: &DatFile,
    pack: Option<&BTreeSet<i32>>,
) -> Vec<super::BuildingComposition> {
    file.units
        .iter()
        .filter(|u| u.building.is_some())
        .filter_map(|u| file.building_composition(u.id).ok())
        .map(|mut b| {
            if let Some(pack) = pack {
                for part in b.parts.iter_mut().chain(&mut b.construction) {
                    part.available = pack.contains(&part.slp_id);
                }
            }
            b
        })
        .collect()
}

pub(super) fn markdown(table: &Table, out: &mut String) {
    out.push_str("## Building composition (DAT order; native pixels)\n\n");
    for (label, id) in super::table::DARK_AGE {
        let Some(recipe) = table.buildings.iter().find(|b| b.unit_id == *id) else {
            continue;
        };
        let _ = writeln!(
            out,
            "### {label}, unit {id}: footprint {:?}, foundation terrain {} → SLP {:?}\n",
            recipe.footprint, recipe.foundation_terrain, recipe.foundation_slp
        );
        if let Some((_, record)) = table.building_records.iter().find(|(unit, _)| unit == id) {
            let _ = writeln!(
                out,
                "- building section at byte {}; construction {}, adjacent {}, angle {}, stack {}, head {}, transform {}",
                record.byte_offset,
                record.construction_graphic,
                record.adjacent_mode,
                record.graphics_angle,
                record.stack_unit,
                record.head_unit,
                record.transform_unit
            );
            for (index, annex) in record.annexes.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "- annex {index}: unit {}, tile misplacement {:?}",
                    annex.unit_id, annex.misplacement
                );
            }
        }
        for (index, part) in recipe.parts.iter().enumerate() {
            let _ = writeln!(
                out,
                "- draw {index}: graphic {} / SLP {}, layer {}, pixel {:?}, tiles {:?}, present {}",
                part.graphic_id,
                part.slp_id,
                part.layer,
                part.offset,
                part.misplacement,
                part.available
            );
        }
        out.push('\n');
    }
    out.push_str("## Graphic deltas (stored list order)\n\n");
    for row in &table.rows {
        for graphic in row
            .graphics
            .iter()
            .chain(&row.family)
            .filter(|g| !g.deltas.is_empty())
        {
            let _ = writeln!(
                out,
                "- graphic {} {}: {:?}",
                graphic.graphic, graphic.name, graphic.deltas
            );
        }
    }
}
