use crate::ast::expressions::Expression;
use crate::ast::extensions::{Extension, TypeOf};
use crate::ast::lexical::{Identifier, StringLiteral};
use crate::ast::statements::Statement;
use crate::span::Node;

// From 6.7 Declarations

#[derive(Debug, PartialEq, Clone)]
pub struct Declaration {
    pub specifiers: Vec<Node<DeclarationSpecifier>>,
    pub declarators: Vec<Node<InitDeclarator>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum DeclarationSpecifier {
    StorageClass(Node<StorageClassSpecifier>),
    TypeSpecifier(Node<TypeSpecifier>),
    TypeQualifier(Node<TypeQualifier>),
    Function(Node<FunctionSpecifier>),
    Alignment(Node<AlignmentSpecifier>),
    Extension(Vec<Node<Extension>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct InitDeclarator {
    pub declarator: Node<Declarator>,
    pub initializer: Option<Node<Initializer>>,
}

// From 6.7.1

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum StorageClassSpecifier {
    Typedef,
    Extern,
    Static,
    ThreadLocal,
    Auto,
    Register,
    Constexpr,
}

// From 6.7.2

#[derive(Debug, PartialEq, Clone)]
pub enum TypeSpecifier {
    Void,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Signed,
    Unsigned,
    Bool,
    Complex,
    Atomic(Node<TypeName>),
    Struct(Node<StructType>),
    Enum(Node<EnumType>),
    TypedefName(Node<Identifier>),
    TypeOf(Node<TypeOf>),
    TS18661Float(TS18661FloatType),
    Int128,
    Float128,
    BitInt(Box<Node<Expression>>),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct TS18661FloatType {
    pub format: TS18661FloatFormat,
    pub width: usize,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TS18661FloatFormat {
    BinaryInterchange,
    BinaryExtended,
    DecimalInterchange,
    DecimalExtended,
}

// From 6.7.2.1

#[derive(Debug, PartialEq, Clone)]
pub struct StructType {
    pub kind: Node<StructKind>,
    pub identifier: Option<Node<Identifier>>,
    pub declarations: Option<Vec<Node<StructDeclaration>>>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum StructKind {
    Struct,
    Union,
}

#[derive(Debug, PartialEq, Clone)]
pub enum StructDeclaration {
    Field(Node<StructField>),
    StaticAssert(Node<StaticAssert>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructField {
    pub specifiers: Vec<Node<SpecifierQualifier>>,
    pub declarators: Vec<Node<StructDeclarator>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum SpecifierQualifier {
    TypeSpecifier(Node<TypeSpecifier>),
    TypeQualifier(Node<TypeQualifier>),
    Extension(Vec<Node<Extension>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructDeclarator {
    pub declarator: Option<Node<Declarator>>,
    pub bit_width: Option<Box<Node<Expression>>>,
}

// From 6.7.2.2

#[derive(Debug, PartialEq, Clone)]
pub struct EnumType {
    pub identifier: Option<Node<Identifier>>,
    pub enumerators: Vec<Node<Enumerator>>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Enumerator {
    pub identifier: Node<Identifier>,
    pub expression: Option<Box<Node<Expression>>>,
    pub extensions: Vec<Node<Extension>>,
}

// From 6.7.3

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TypeQualifier {
    Const,
    Restrict,
    Volatile,
    Nonnull,
    NullUnspecified,
    Nullable,
    Atomic,
}

// From 6.7.4

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum FunctionSpecifier {
    Inline,
    Noreturn,
}

// From 6.7.5

#[derive(Debug, PartialEq, Clone)]
pub enum AlignmentSpecifier {
    Type(Node<TypeName>),
    Constant(Box<Node<Expression>>),
}

// From 6.7.6 Declarators

#[derive(Debug, PartialEq, Clone)]
pub struct Declarator {
    pub kind: Node<DeclaratorKind>,
    pub derived: Vec<Node<DerivedDeclarator>>,
    pub extensions: Vec<Node<Extension>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum DeclaratorKind {
    Abstract,
    Identifier(Node<Identifier>),
    Declarator(Box<Node<Declarator>>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum DerivedDeclarator {
    Pointer(Vec<Node<PointerQualifier>>),
    Array(Node<ArrayDeclarator>),
    Function(Node<FunctionDeclarator>),
    KRFunction(Vec<Node<Identifier>>),
    Block(Vec<Node<PointerQualifier>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct ArrayDeclarator {
    pub qualifiers: Vec<Node<TypeQualifier>>,
    pub size: ArraySize,
}

#[derive(Debug, PartialEq, Clone)]
pub struct FunctionDeclarator {
    pub parameters: Vec<Node<ParameterDeclaration>>,
    pub ellipsis: Ellipsis,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PointerQualifier {
    TypeQualifier(Node<TypeQualifier>),
    Extension(Vec<Node<Extension>>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ArraySize {
    Unknown,
    VariableUnknown,
    VariableExpression(Box<Node<Expression>>),
    StaticExpression(Box<Node<Expression>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct ParameterDeclaration {
    pub specifiers: Vec<Node<DeclarationSpecifier>>,
    pub declarator: Option<Node<Declarator>>,
    pub extensions: Vec<Node<Extension>>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Ellipsis {
    Some,
    None,
}

// From 6.7.7 Type names

#[derive(Debug, PartialEq, Clone)]
pub struct TypeName {
    pub specifiers: Vec<Node<SpecifierQualifier>>,
    pub declarator: Option<Node<Declarator>>,
}

// From 6.7.9 Initialization

#[derive(Debug, PartialEq, Clone)]
pub enum Initializer {
    Expression(Box<Node<Expression>>),
    List(Vec<Node<InitializerListItem>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct InitializerListItem {
    pub designation: Vec<Node<Designator>>,
    pub initializer: Box<Node<Initializer>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Designator {
    Index(Node<Expression>),
    Member(Node<Identifier>),
    Range(Node<RangeDesignator>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct RangeDesignator {
    pub from: Node<Expression>,
    pub to: Node<Expression>,
}

// From 6.7.10 Static assertions

#[derive(Debug, PartialEq, Clone)]
pub struct StaticAssert {
    pub expression: Box<Node<Expression>>,
    pub message: Node<StringLiteral>,
}

// From 6.9 External definitions

#[derive(Debug, PartialEq, Clone)]
pub struct TranslationUnit(pub Vec<Node<ExternalDeclaration>>);

#[derive(Debug, PartialEq, Clone)]
// Boxing the largest variant would break the stable public AST representation.
#[allow(clippy::large_enum_variant)]
pub enum ExternalDeclaration {
    Declaration(Node<Declaration>),
    StaticAssert(Node<StaticAssert>),
    FunctionDefinition(Node<FunctionDefinition>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct FunctionDefinition {
    pub specifiers: Vec<Node<DeclarationSpecifier>>,
    pub declarator: Node<Declarator>,
    pub declarations: Vec<Node<Declaration>>,
    pub statement: Node<Statement>,
}
