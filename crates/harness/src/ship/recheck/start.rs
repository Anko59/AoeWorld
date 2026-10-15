//! Synchronize the selected base before any ship gate or review.
use super::{Options, Result, git, rebase, short};
use std::path::Path;

pub(in crate::ship) fn prepare(root: &Path, options: &Options) -> Result<String> {
    let subject = git::subject(root)?;
    let (base, merge_base) = git::ship_base(root, &options.base, !options.no_fetch)?;
    if base != merge_base {
        // Dev is never rewritten. A stacked parent's reflog identifies its
        // old tip, so superseded parent commits are not replayed as child work.
        let boundary = if options.base == crate::review::base::DEV {
            merge_base
        } else {
            git::git(
                root,
                &[
                    "merge-base",
                    "--fork-point",
                    &crate::review::base::reference(&options.base),
                    "HEAD",
                ],
            )
            .map_err(|_| {
                format!(
                    "ship: cannot establish the fork point of {}; rebase the child onto {} \
                     yourself, then ship; nothing was pushed",
                    options.base, options.base
                )
            })?
        };
        rebase(
            root,
            options,
            &base,
            &boundary,
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
