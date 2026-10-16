use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Operation {
    FmtCheck,
    StructureCheck,
    ArchitectureCheck,
    DocsCheck,
    Lint,
    TestUnit,
}
impl Operation {
    pub(crate) fn argument(&self) -> &'static str {
        match self {
            Self::FmtCheck => "fmt-check",
            Self::StructureCheck => "structure-check",
            Self::ArchitectureCheck => "architecture-check",
            Self::DocsCheck => "docs-check",
            Self::Lint => "lint",
            Self::TestUnit => "test-unit",
        }
    }
}
