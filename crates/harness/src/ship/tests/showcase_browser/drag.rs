use super::*;

fn board_with_drag(viewport: &str, from: (u32, u32), to: (u32, u32)) -> String {
    format!(
        r##"{{"title":"Lab","viewport":"{viewport}","scenes":[
          {{"kind":"card","heading":"Opening"}},
          {{"kind":"browser","path":"/","caption":"The lab","seconds":3,"steps":[
            {{"drag":{{"selector":"#scene","from":{{"x":{},"y":{}}},"to":{{"x":{},"y":{}}}}}}}
          ]}}
        ]}}"##,
        from.0, from.1, to.0, to.1
    )
}

#[test]
fn a_drag_must_land_on_the_page_it_is_filmed_on() {
    // The default page is 1280×720: an offset past it cannot land, even
    // when the element is somewhere else on the page.
    for (from, to) in [
        ((2000, 100), (2100, 200)),
        ((10, 10), (10, 720)),
        ((1280, 0), (1, 1)),
    ] {
        let error = Storyboard::parse(&board_with_drag("default", from, to)).unwrap_err();
        assert!(error.contains("drag points must land"), "{error}");
        assert!(error.contains("1280×720"), "{error}");
        assert!(error.contains("scene 2"), "{error}");
        assert!(error.contains("step 1"), "{error}");
    }
    // The last pixel of the page is still on it.
    Storyboard::parse(&board_with_drag("default", (1279, 719), (0, 0))).unwrap();
    // `large` films a 1920×1080 page, so offsets up to that size are valid.
    Storyboard::parse(&board_with_drag("large", (1500, 900), (1919, 1079))).unwrap();
    let error = Storyboard::parse(&board_with_drag("large", (1920, 0), (1, 1))).unwrap_err();
    assert!(error.contains("1920×1080"), "{error}");
}

#[test]
fn the_recorder_refuses_a_drag_outside_the_element_or_the_page() {
    let script = include_str!("../../showcase/record.mjs");
    let check = script
        .find("const onElement = point.x < box.width")
        .expect("the recorder checks drag points");
    let press = script
        .find("await page.mouse.down();")
        .expect("the recorder presses the mouse");
    assert!(
        check < press,
        "the drag must be checked before the mouse moves"
    );
    assert!(script.contains("const view = page.viewportSize();"));
    assert!(script.contains("is outside ${selector}"));
}
