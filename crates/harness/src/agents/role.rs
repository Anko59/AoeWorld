//! A role comes from two sources only: `AOE_AGENT_ROLE`, set by the harness when
//! it launches an agent (hook processes inherit it; the agent cannot change it),
//! and the hook's `agent_type`, so each Claude agent file's `name` is its role.
//! When both name a role, the narrower one applies. Protected classes are edited by the main session
//! only, where a person is present; `.github/CODEOWNERS` names the same paths.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// The person's own session (no `agent_type`).
    Main,
    /// Writes failing tests only.
    Tester,
    /// Writes production code only, never a test.
    Implementer,
    /// Reads and reports; never writes.
    Reviewer,
    /// Any other agent: a developer outside the protected classes.
    Other,
}

impl Role {
    pub(crate) fn from_agent(agent_type: Option<&str>) -> Self {
        let lower = agent_type.map(|name| name.trim().to_ascii_lowercase());
        match lower.as_deref() {
            None | Some("" | "main") => Self::Main,
            Some("tester") => Self::Tester,
            Some("implementer" | "coder") => Self::Implementer,
            Some("reviewer" | "verifier" | "qa" | "explore" | "plan" | "claude-code-guide") => {
                Self::Reviewer
            }
            Some(name) if name.starts_with("reviewer-") || name.starts_with("rv-") => {
                Self::Reviewer
            }
            Some(_) => Self::Other,
        }
    }

    /// The role for a call: a launched role and a reported agent type each
    /// restrict, and when both are set the narrower one wins. An empty or
    /// `main` launched role restricts nothing.
    pub(crate) fn resolve(launched: Option<&str>, agent_type: Option<&str>) -> Self {
        Self::from_agent(launched).narrower(Self::from_agent(agent_type))
    }

    fn narrower(self, other: Self) -> Self {
        match (self, other) {
            (a, b) if a == b => a,
            (Self::Main, b) => b,
            (a, Self::Main) => a,
            (Self::Other, b) => b,
            (a, Self::Other) => a,
            // Tester and implementer write disjoint files; together, nothing.
            _ => Self::Reviewer,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Main => "main session",
            Self::Tester => "tester",
            Self::Implementer => "implementer",
            Self::Reviewer => "reviewer",
            Self::Other => "agent",
        }
    }

    pub(crate) fn is_agent(self) -> bool {
        self != Self::Main
    }

    /// Tester, Implementer and Reviewer never commit, push or move Git state:
    /// the session that ran them reviews and ships.
    pub(crate) fn duty_bound(self) -> bool {
        matches!(self, Self::Tester | Self::Implementer | Self::Reviewer)
    }
}

pub(crate) struct Class {
    pub(crate) name: &'static str,
    pub(crate) remedy: &'static str,
}

const HARNESS: Class = Class {
    name: "harness policy",
    remedy: "Report the problem to the main session instead of changing the rules that judge you.",
};
const GATES: Class = Class {
    name: "gates",
    remedy: "Never lower a gate, floor or limit to pass; report the failure instead.",
};
const BASELINES: Class = Class {
    name: "baselines",
    remedy: "Baselines change only through their reviewed proposal targets (for example `make perf-baseline-propose`).",
};

/// Repository-relative, lower-case, `/`-separated path → its protected class.
pub(crate) fn protected(relative: &str) -> Option<Class> {
    const HARNESS_PREFIXES: [&str; 10] = [
        "crates/harness/",
        "make/",
        ".agents/",
        ".claude/",
        ".codex/",
        ".dsh/",
        ".pi/",
        "docker/",
        "skills/",
        "docs/adr/",
    ];
    const HARNESS_FILES: [&str; 10] = [
        "claude.md",
        "agents.md",
        "makefile",
        "cargo.toml",
        "rust-toolchain.toml",
        "deny.toml",
        ".gitignore",
        ".dockerignore",
        "docs/agent-engineering.md",
        "docs/agent-runtimes.md",
    ];
    let under =
        |prefix: &str| relative == prefix.trim_end_matches('/') || relative.starts_with(prefix);
    if HARNESS_PREFIXES.iter().any(|p| under(p))
        || HARNESS_FILES.contains(&relative)
        || relative.ends_with("/agents.md")
    {
        Some(HARNESS)
    } else if under("gates/") || under(".github/") {
        Some(GATES)
    } else if under("baselines/") {
        Some(BASELINES)
    } else {
        None
    }
}

/// Test files by this repository's conventions: `tests.rs`, `tests/` modules,
/// `*_tests.rs`, browser specs, fuzz targets and fixtures.
pub(crate) fn is_test(relative: &str) -> bool {
    let mut parts: Vec<&str> = relative.split('/').collect();
    let Some(file) = parts.pop() else {
        return false;
    };
    parts
        .iter()
        .any(|dir| matches!(*dir, "tests" | "testdata" | "fixtures" | "fuzz_targets"))
        || relative.starts_with("fuzz/")
        || matches!(file, "tests" | "tests.rs")
        || file.ends_with("_tests.rs")
        || file.ends_with("_test.rs")
        || file.ends_with(".spec.ts")
        || file.ends_with(".test.ts")
}
