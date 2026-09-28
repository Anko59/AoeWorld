use super::*;

#[test]
fn split_allocation_subpages_reassemble_original_row_major_page() {
    let page = PageBounds {
        x: 11,
        y: 7,
        width: 5,
        height: 3,
    };
    let children = split_page_bounds(page).expect("nontrivial page splits");
    let mut output = vec![usize::MAX; page_cell_count(page)];

    for child in children {
        let values = (0..child.height)
            .flat_map(|row| {
                (0..child.width).map(move |column| {
                    usize::from(child.y + row) * 1_000 + usize::from(child.x + column)
                })
            })
            .collect::<Vec<_>>();
        place_child_values(page, child, &values, &mut output);
    }

    for row in 0..page.height {
        for column in 0..page.width {
            let index = usize::from(row) * usize::from(page.width) + usize::from(column);
            assert_eq!(
                output[index],
                usize::from(page.y + row) * 1_000 + usize::from(page.x + column)
            );
        }
    }
}
