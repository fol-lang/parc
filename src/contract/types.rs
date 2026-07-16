use serde::{Deserialize, Serialize};

use super::{DeclarationId, DiagnosticCode, SourceRange, SupportStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CType {
    pub qualifiers: TypeQualifiers,
    pub nullability: Nullability,
    pub kind: CTypeKind,
    pub support: SupportStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeQualifiers {
    pub is_const: bool,
    pub is_volatile: bool,
    pub is_restrict: bool,
    pub is_atomic: bool,
}

impl TypeQualifiers {
    pub const NONE: Self = Self {
        is_const: false,
        is_volatile: false,
        is_restrict: false,
        is_atomic: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nullability {
    Unspecified,
    Nonnull,
    Nullable,
    NullUnspecified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CTypeKind {
    Void,
    Bool,
    Integer(CIntegerType),
    Floating(CFloatingType),
    Complex(CFloatingType),
    Pointer(Box<CType>),
    Array {
        element: Box<CType>,
        bound: ArrayBound,
        /// Qualifiers written inside a function-parameter array declarator,
        /// which qualify the pointer produced by C parameter adjustment.
        parameter_qualifiers: TypeQualifiers,
    },
    Function(CFunctionType),
    AliasRef(DeclarationId),
    RecordRef(DeclarationId),
    EnumRef(DeclarationId),
    Unsupported {
        category: UnsupportedTypeCategory,
        spelling: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rank", rename_all = "snake_case", deny_unknown_fields)]
pub enum CIntegerType {
    Char {
        signedness: CharTypeSignedness,
    },
    Short {
        signedness: Signedness,
    },
    Int {
        signedness: Signedness,
    },
    Long {
        signedness: Signedness,
    },
    LongLong {
        signedness: Signedness,
    },
    Int128 {
        signedness: Signedness,
    },
    BitInt {
        signedness: Signedness,
        width: BitIntWidth,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharTypeSignedness {
    Plain,
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signedness {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BitIntWidth {
    Known { bits: u64 },
    Expression { normalized_expression: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CFloatingType {
    Float,
    Double,
    LongDouble,
    Float128,
    Ts18661 { format: Ts18661Format, width: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ts18661Format {
    BinaryInterchange,
    BinaryExtended,
    DecimalInterchange,
    DecimalExtended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArrayBound {
    Fixed {
        elements: u64,
    },
    Incomplete,
    Flexible,
    Variable {
        normalized_expression: String,
    },
    StaticMinimum {
        minimum: ArrayMinimumBound,
    },
    Invalid {
        spelling: String,
        diagnostic: DiagnosticCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArrayMinimumBound {
    Fixed { elements: u64 },
    Variable { normalized_expression: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CFunctionType {
    pub return_type: Box<CType>,
    pub parameters: Vec<CFunctionParameter>,
    pub prototype: FunctionPrototype,
    pub calling_convention: CallingConvention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CFunctionParameter {
    pub name: Option<String>,
    pub ty: CType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FunctionPrototype {
    Prototyped { variadic: bool },
    UnspecifiedParameters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallingConvention {
    C,
    Cdecl,
    Stdcall,
    Fastcall,
    Vectorcall,
    Thiscall,
    SysV64,
    Win64,
    Aapcs,
    Unsupported { spelling: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedTypeCategory {
    Typeof,
    Vector,
    BlockPointer,
    ComplexRepresentation,
    CompilerBuiltin,
    InvalidSpecifierCombination,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAttribute {
    pub namespace: Option<String>,
    pub name: String,
    pub arguments: Vec<String>,
    pub spelling: String,
    pub range: SourceRange,
    pub disposition: AttributeDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeDisposition {
    Modeled,
    Preserved,
    UnsupportedAbiRelevant,
}
