use serde::{Deserialize, Serialize};

use super::{
    ExactInteger, FileId, MacroId, OccurrenceId, SourceProvenance, SourceRange, SupportStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMacro {
    pub id: MacroId,
    pub identity_file: FileId,
    pub name: String,
    pub form: MacroForm,
    pub category: MacroCategory,
    pub body: String,
    pub normalized_tokens: Vec<String>,
    pub value: Option<MacroValue>,
    pub occurrences: Vec<MacroOccurrence>,
    pub support: SupportStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroForm {
    ObjectLike,
    FunctionLike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MacroCategory {
    BindableConstant,
    ConfigurationFlag,
    AbiAffecting,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MacroValue {
    Integer { value: ExactInteger },
    String { value: String },
    Tokens { tokens: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroOccurrence {
    pub id: OccurrenceId,
    pub range: SourceRange,
    pub normalized_tokens: Vec<String>,
    pub duplicate_ordinal: u64,
    pub provenance: SourceProvenance,
}
