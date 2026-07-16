use serde::{Deserialize, Serialize};

use super::SourceRange;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProvenance {
    pub origin: SourceOrigin,
    pub include_chain: Vec<IncludeSite>,
    pub macro_expansions: Vec<MacroExpansion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    Entry,
    UserInclude,
    SystemInclude,
    Builtin,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeSite {
    pub directive: SourceRange,
    pub included: super::FileId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroExpansion {
    pub macro_name: String,
    pub invocation: SourceRange,
    pub definition: Option<SourceRange>,
}
