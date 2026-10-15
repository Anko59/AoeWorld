//! Synchronize the selected base before any ship gate or review.
use super::{Options, Result, git, rebase, short};
use std::path::Path;

pub(in crate::ship) fn prepare(root: &Path, options: &Options) -> Result<String> {
    let subject = git::subject(root)?;
    let (base, merge_base) = git::ship_base(root, &options.base, !options.no_fetch)?;
    if base != merge_base {
        rebase(
            root,
            options,
            &base,
            &merge_base,
            &subject.head,
            "was ahead before the gates",
        )?;
        eprintln!(
            "ship: {} was ahead at start; rebased onto {} before the gates and review",
            options.base,
            short(&base)
        );
    }
    Ok(base)
}
