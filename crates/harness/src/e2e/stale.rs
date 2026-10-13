//! A stored map package from before a format change fails the browser stack's
//! startup validation. The product error stays first; this adds the way out.
use std::path::Path;

#[cfg(test)]
mod tests;

const MARKER: &str = "invalid stored map package ";
const NAMED_FILES: usize = 3;

/// Keeps `error` unchanged unless its text names an invalid stored package.
pub(super) fn annotate(
    error: Box<dyn std::error::Error>,
    directory: &Path,
) -> Box<dyn std::error::Error> {
    let message = error.to_string();
    match hint(&message, directory) {
        Some(hint) => format!("{message}\n{hint}").into(),
        None => error,
    }
}

fn hint(message: &str, directory: &Path) -> Option<String> {
    let mut files: Vec<&str> = Vec::new();
    for (start, _) in message.match_indices(MARKER) {
        let rest = &message[start + MARKER.len()..];
        let end = rest.find(".json").map_or_else(
            || rest.find(": ").unwrap_or(rest.len()),
            |index| index + ".json".len(),
        );
        let file = &rest[..end];
        if !files.contains(&file) {
            files.push(file);
        }
    }
    if files.is_empty() {
        return None;
    }
    let more = files.len().saturating_sub(NAMED_FILES);
    let mut named = files[..files.len() - more].join(", ");
    if more > 0 {
        named.push_str(&format!(" +{more} more"));
    }
    Some(format!(
        "invalid stored map package files: {named}\n\
         stale local data from before a format change: regenerate the packages, or move {} aside \
         (for example `mv local-assets/map-packages local-assets/map-packages.stale`); CI has none",
        directory.display()
    ))
}
