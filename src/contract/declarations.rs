use serde::{Deserialize, Serialize};

use super::{
    CType, CallingConvention, ChildId, DeclarationId, DiagnosticCode, ExactInteger,
    FunctionPrototype, OccurrenceId, SourceAttribute, SourceProvenance, SourceRange, SupportStatus,
};
use super::{ContentFingerprint, EntityNamespace, EntityScope};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDeclaration {
    pub id: DeclarationId,
    pub identity: DeclarationIdentity,
    pub name: Option<SourceName>,
    pub linkage: Linkage,
    pub visibility: Visibility,
    pub occurrences: Vec<DeclarationOccurrence>,
    pub support: SupportStatus,
    pub kind: SourceDeclarationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeclarationIdentity {
    Named {
        namespace: EntityNamespace,
        scope: EntityScope,
        normalized_name: String,
    },
    Anonymous {
        scope: EntityScope,
        token_fingerprint: ContentFingerprint,
        duplicate_ordinal: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceName {
    pub normalized: String,
    pub original: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Linkage {
    External,
    Internal,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// No explicit source visibility and no proven target/compiler default.
    Unspecified,
    /// An explicit source `visibility("default")` request.
    ExplicitDefault,
    /// Default visibility proven from the effective target/compiler inputs.
    TargetDefault,
    Hidden,
    Protected,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationOccurrence {
    pub id: OccurrenceId,
    pub range: SourceRange,
    pub name_range: Option<SourceRange>,
    pub spelling: String,
    pub normalized_tokens: Vec<String>,
    pub duplicate_ordinal: u64,
    pub storage: StorageClass,
    pub is_definition: bool,
    pub attributes: Vec<SourceAttribute>,
    pub provenance: SourceProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageClass {
    None,
    Typedef,
    Extern,
    Static,
    ThreadLocal,
    Auto,
    Register,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SourceDeclarationKind {
    Function(SourceFunction),
    Record(SourceRecord),
    Enum(SourceEnum),
    TypeAlias(SourceTypeAlias),
    Variable(SourceVariable),
    Unsupported(SourceUnsupported),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFunction {
    pub link_name: String,
    pub return_type: CType,
    pub parameters: Vec<SourceParameter>,
    pub prototype: FunctionPrototype,
    pub calling_convention: CallingConvention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceParameter {
    pub id: ChildId,
    /// Zero-based semantic prototype position. Parameter names are deliberately
    /// excluded from identity because C redeclarations may rename or omit them.
    pub ordinal: u64,
    pub name: Option<SourceName>,
    pub ty: CType,
    pub range: SourceRange,
    pub provenance: SourceProvenance,
    pub attributes: Vec<SourceAttribute>,
    pub support: SupportStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Struct,
    Union,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordCompleteness {
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub kind: RecordKind,
    pub completeness: RecordCompleteness,
    pub fields: Vec<SourceField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceField {
    pub id: ChildId,
    pub name: Option<SourceName>,
    pub ty: CType,
    pub bit_width: Option<BitWidth>,
    pub range: SourceRange,
    pub provenance: SourceProvenance,
    pub attributes: Vec<SourceAttribute>,
    pub support: SupportStatus,
    pub identity_tokens: Vec<String>,
    pub duplicate_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BitWidth {
    Known {
        bits: u64,
    },
    Expression {
        normalized_expression: String,
    },
    Invalid {
        spelling: String,
        diagnostic: DiagnosticCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEnum {
    pub explicit_underlying_type: Option<CType>,
    pub variants: Vec<SourceEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEnumVariant {
    pub id: ChildId,
    pub name: SourceName,
    pub value: EnumValue,
    pub range: SourceRange,
    pub provenance: SourceProvenance,
    pub attributes: Vec<SourceAttribute>,
    pub support: SupportStatus,
    pub identity_tokens: Vec<String>,
    pub duplicate_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnumValue {
    Evaluated { value: ExactInteger },
    Unevaluated { normalized_expression: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTypeAlias {
    pub target: CType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVariable {
    pub link_name: String,
    pub ty: CType,
    pub thread_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceUnsupported {
    pub category: UnsupportedDeclarationCategory,
    pub spelling: String,
    pub diagnostic: DiagnosticCode,
}

/// Closed classification for declarations that cannot be represented as a
/// typed source declaration. Exact source spelling remains available on
/// [`SourceUnsupported`] for diagnostics, never as a dispatch key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedDeclarationCategory {
    StaticAssertion,
    InlineAssembly,
    CompilerBuiltin,
    UnsupportedExtension,
    InvalidDeclaration,
}
