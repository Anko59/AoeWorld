//! The legs of `preflight`, which CI runs in parallel in its `static` job (issue #388),
//! chosen by the selection manifest (`static_parts`); locally `all` keeps the single
//! sequential run.

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
    /// The static checks that compile nothing, for a change without Rust code.
    Light,
}

impl PreflightPart {
    pub(crate) fn runs_checks(self) -> bool {
        matches!(self, Self::All | Self::Checks)
    }

    pub(crate) fn runs_light(self) -> bool {
        matches!(self, Self::Light)
    }

    pub(crate) fn runs_unit(self) -> bool {
        matches!(self, Self::All | Self::Unit)
    }
}
