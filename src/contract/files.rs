use serde::{Deserialize, Serialize};

use super::{ContentFingerprint, FileId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileRole {
    Entry,
    UserInclude,
    SystemInclude,
    Builtin,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub id: FileId,
    pub logical_path: String,
    pub role: SourceFileRole,
    pub content: ContentFingerprint,
    pub byte_len: u64,
    pub line_starts: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRange {
    pub file: FileId,
    pub start: u64,
    pub end: u64,
}
