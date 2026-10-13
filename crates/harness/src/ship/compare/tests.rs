use crate::ship::compare::bookmarks::Bookmarks;

mod bookmarks;
mod pixels;
mod settings;
mod views;

// Shared by the test modules.
pub(super) const MAPS: &str = r#""maps": {"france": {"center_latitude_e7": 462000000, "center_longitude_e7": 22000000, "requested_side_meters": 1200000}}"#;

pub(super) fn mark(name: &str) -> String {
    format!(r#"{{"name": "{name}", "map": "france", "x": 10.5, "y": 20, "zoom": 1}}"#)
}

pub(super) fn file(marks: &[String]) -> String {
    format!(r#"{{{MAPS}, "bookmarks": [{}]}}"#, marks.join(","))
}

pub(super) fn parse(marks: &[String]) -> Result<Bookmarks, String> {
    Bookmarks::parse(&file(marks))
}
