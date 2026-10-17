//! The two halves of `preflight`, which CI runs as parallel legs of its
//! `static` job (issue #388); locally `all` keeps the single sequential run.

/// Which gates of the preflight a run executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum PreflightPart {
    /// Every gate, in order: what `make preflight` runs.
    All,
    /// The static checks and clippy (the `deny` and `build-wasm` gates run
    /// as Make prerequisites of `make preflight-checks`).
    Checks,
    /// The native unit tests, then the smoke workload that needs them.
    Unit,
}

impl PreflightPart {
    pub(crate) fn runs_checks(self) -> bool {
        matches!(self, Self::All | Self::Checks)
    }

    pub(crate) fn runs_unit(self) -> bool {
        matches!(self, Self::All | Self::Unit)
    }
}
