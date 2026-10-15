//! Docker's --mount is CSV: colons are path data and quoted fields also
//! preserve commas and embedded quotes. The shell never parses this value.
use std::path::Path;

pub(super) fn bind(source: &Path, target: &Path, readonly: bool) -> String {
    let field = |name: &str, path: &Path| {
        format!("\"{name}={}\"", path.to_string_lossy().replace('"', "\"\""))
    };
    format!(
        "type=bind,{},{}{}",
        field("source", source),
        field("target", target),
        if readonly { ",readonly" } else { "" }
    )
}
