use serde::{Deserialize, Serialize};

use super::{ContentFingerprint, FileId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveSourceInputs {
    /// Canonical, strictly FileId-ordered set of translation-unit entries.
    pub entry_files: Vec<FileId>,
    /// Ordered search sequence; repetition is semantic and preserved.
    pub include_search: Vec<IncludeSearchEntry>,
    /// Ordered command-line event stream; repetition is semantic and preserved.
    pub define_events: Vec<DefineEvent>,
    /// Ordered forced-include sequence; repetition is semantic and preserved.
    pub forced_includes: Vec<FileId>,
    pub preprocessor: PreprocessorIdentity,
    pub environment: EnvironmentInputs,
    pub path_mapping_fingerprint: ContentFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncludeSearchEntry {
    pub logical_path: String,
    pub kind: IncludeSearchKind,
    pub content: Option<ContentFingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncludeSearchKind {
    Quote,
    User,
    System,
    Framework,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefineEvent {
    Define { name: String, value: Option<String> },
    Undefine { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreprocessorIdentity {
    Builtin {
        implementation_version: String,
    },
    External {
        executable: String,
        executable_fingerprint: ContentFingerprint,
        /// Ordered argv suffix; repetition is semantic and preserved.
        arguments: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnvironmentInputs {
    Hermetic,
    /// Canonical, strictly name-ordered captured environment set.
    Captured {
        variables: Vec<CapturedEnvironment>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedEnvironment {
    pub name: String,
    pub value_fingerprint: ContentFingerprint,
}
