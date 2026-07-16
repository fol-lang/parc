use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::contract::{
    ArrayBound, ArrayMinimumBound, BitIntWidth, CFloatingType, CFunctionParameter, CFunctionType,
    CIntegerType, CType, CTypeKind, CallingConvention, CharTypeSignedness, DeclarationId,
    DiagnosticCode, ExactInteger, FunctionPrototype, Nullability, Signedness, SupportStatus,
    Ts18661Format, TypeQualifiers, UnsupportedTypeCategory,
};
use crate::span::{Node, Span};

pub(crate) struct TypeResolver<'a> {
    ordinary: &'a BTreeMap<String, DeclarationId>,
    tags: &'a BTreeMap<String, DeclarationId>,
    anonymous_tags: &'a BTreeMap<(usize, usize), DeclarationId>,
    pointer_aliases: &'a BTreeSet<DeclarationId>,
    source: &'a str,
}

pub(crate) struct LoweredParameter {
    pub(crate) name: Option<String>,
    pub(crate) ty: CType,
    pub(crate) span: Span,
}

pub(crate) struct LoweredFunctionParts {
    pub(crate) return_type: CType,
    pub(crate) parameters: Vec<LoweredParameter>,
    pub(crate) prototype: FunctionPrototype,
}

impl<'a> TypeResolver<'a> {
    pub(crate) fn new(
        ordinary: &'a BTreeMap<String, DeclarationId>,
        tags: &'a BTreeMap<String, DeclarationId>,
        anonymous_tags: &'a BTreeMap<(usize, usize), DeclarationId>,
        pointer_aliases: &'a BTreeSet<DeclarationId>,
        source: &'a str,
    ) -> Self {
        Self {
            ordinary,
            tags,
            anonymous_tags,
            pointer_aliases,
            source,
        }
    }

    pub(crate) fn declaration_type(
        &self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: Option<&Declarator>,
    ) -> CType {
        let type_specs: Vec<_> = specifiers
            .iter()
            .filter_map(|specifier| match &specifier.node {
                DeclarationSpecifier::TypeSpecifier(specifier) => Some(specifier),
                _ => None,
            })
            .collect();
        let (qualifiers, nullability) = declaration_qualifiers(specifiers);
        self.finish_type(type_specs, declarator, qualifiers, nullability, false)
    }

    pub(crate) fn field_type(
        &self,
        specifiers: &[Node<SpecifierQualifier>],
        declarator: Option<&Declarator>,
    ) -> CType {
        let type_specs: Vec<_> = specifiers
            .iter()
            .filter_map(|specifier| match &specifier.node {
                SpecifierQualifier::TypeSpecifier(specifier) => Some(specifier),
                _ => None,
            })
            .collect();
        let (qualifiers, nullability) = field_qualifiers(specifiers);
        self.finish_type(type_specs, declarator, qualifiers, nullability, false)
    }

    fn parameter_type(
        &self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: Option<&Declarator>,
    ) -> CType {
        let type_specs: Vec<_> = specifiers
            .iter()
            .filter_map(|specifier| match &specifier.node {
                DeclarationSpecifier::TypeSpecifier(specifier) => Some(specifier),
                _ => None,
            })
            .collect();
        let (qualifiers, nullability) = declaration_qualifiers(specifiers);
        self.finish_type(type_specs, declarator, qualifiers, nullability, true)
    }

    fn finish_type(
        &self,
        type_specs: Vec<&Node<TypeSpecifier>>,
        declarator: Option<&Declarator>,
        qualifiers: TypeQualifiers,
        nullability: Nullability,
        parameter_array_allowed: bool,
    ) -> CType {
        let mut ty = self.base_type(&type_specs);
        ty.qualifiers.is_const |= qualifiers.is_const;
        ty.qualifiers.is_volatile |= qualifiers.is_volatile;
        ty.qualifiers.is_atomic |= qualifiers.is_atomic;
        if let Some(declarator) = declarator {
            ty = self.apply_derived(ty, declarator, parameter_array_allowed);
        }
        if qualifiers.is_restrict || nullability != Nullability::Unspecified {
            let effectively_pointer = matches!(ty.kind, CTypeKind::Pointer(_))
                || matches!(ty.kind, CTypeKind::AliasRef(id) if self.pointer_aliases.contains(&id));
            if effectively_pointer {
                ty.qualifiers.is_restrict |= qualifiers.is_restrict;
                ty.nullability = nullability;
            } else {
                return unsupported(
                    UnsupportedTypeCategory::InvalidSpecifierCombination,
                    "non-pointer restrict/nullability qualifier",
                );
            }
        }
        ty
    }

    pub(crate) fn function_parts(
        &self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: &Declarator,
    ) -> Option<LoweredFunctionParts> {
        let mut return_type = self.declaration_type(specifiers, None);
        let mut parameters = None;
        let mut prototype = FunctionPrototype::UnspecifiedParameters;
        for derived in &declarator.derived {
            match &derived.node {
                DerivedDeclarator::Pointer(qualifiers) => {
                    return_type = pointer(return_type, pointer_qualifiers(qualifiers));
                }
                DerivedDeclarator::Array(array) => {
                    return_type = array_type(
                        self.source,
                        return_type,
                        &array.node.size,
                        &array.node.qualifiers,
                        false,
                    );
                }
                DerivedDeclarator::Function(function) => {
                    let lowered = self.parameters(&function.node.parameters);
                    prototype = if function.node.parameters.is_empty() {
                        FunctionPrototype::UnspecifiedParameters
                    } else {
                        FunctionPrototype::Prototyped {
                            variadic: function.node.ellipsis == Ellipsis::Some,
                        }
                    };
                    parameters = Some(lowered);
                }
                DerivedDeclarator::KRFunction(_) | DerivedDeclarator::Block(_) => return None,
            }
        }
        let mut parameters = parameters?;
        if parameters.len() == 1
            && parameters[0].name.is_none()
            && matches!(parameters[0].ty.kind, CTypeKind::Void)
        {
            parameters.clear();
            prototype = FunctionPrototype::Prototyped { variadic: false };
        }
        Some(LoweredFunctionParts {
            return_type,
            parameters,
            prototype,
        })
    }

    fn parameters(&self, parameters: &[Node<ParameterDeclaration>]) -> Vec<LoweredParameter> {
        parameters
            .iter()
            .map(|parameter| {
                let declarator = parameter.node.declarator.as_ref();
                let name = declarator.and_then(|value| declarator_name(&value.node));
                let ty = self.parameter_type(
                    &parameter.node.specifiers,
                    declarator.map(|value| &value.node),
                );
                LoweredParameter {
                    name,
                    ty,
                    span: parameter.span,
                }
            })
            .collect()
    }

    fn apply_derived(
        &self,
        mut ty: CType,
        declarator: &Declarator,
        mut parameter_array_allowed: bool,
    ) -> CType {
        let mut pointers = Vec::new();
        for derived in &declarator.derived {
            match &derived.node {
                DerivedDeclarator::Pointer(qualifiers) => {
                    pointers.push(pointer_qualifiers(qualifiers));
                }
                DerivedDeclarator::Array(array) => {
                    ty = array_type(
                        self.source,
                        ty,
                        &array.node.size,
                        &array.node.qualifiers,
                        parameter_array_allowed,
                    );
                    parameter_array_allowed = false;
                }
                DerivedDeclarator::Function(function) => {
                    let mut parameters = self.parameters(&function.node.parameters);
                    if parameters.len() == 1
                        && parameters[0].name.is_none()
                        && matches!(parameters[0].ty.kind, CTypeKind::Void)
                    {
                        parameters.clear();
                    }
                    let parameter_types = parameters
                        .into_iter()
                        .map(|parameter| CFunctionParameter {
                            name: parameter.name,
                            ty: parameter.ty,
                        })
                        .collect();
                    ty = supported(CTypeKind::Function(CFunctionType {
                        return_type: Box::new(ty),
                        parameters: parameter_types,
                        prototype: if function.node.parameters.is_empty() {
                            FunctionPrototype::UnspecifiedParameters
                        } else {
                            FunctionPrototype::Prototyped {
                                variadic: function.node.ellipsis == Ellipsis::Some,
                            }
                        },
                        calling_convention: declarator_calling_convention(declarator),
                    }));
                }
                DerivedDeclarator::KRFunction(_) => {
                    return unsupported(UnsupportedTypeCategory::Other, "K&R function declarator");
                }
                DerivedDeclarator::Block(_) => {
                    return unsupported(
                        UnsupportedTypeCategory::BlockPointer,
                        "block pointer declarator",
                    );
                }
            }
        }
        for qualifiers in pointers {
            ty = pointer(ty, qualifiers);
        }
        if let DeclaratorKind::Declarator(inner) = &declarator.kind.node {
            ty = self.apply_derived(ty, &inner.node, parameter_array_allowed);
        }
        ty
    }

    fn base_type(&self, type_specs: &[&Node<TypeSpecifier>]) -> CType {
        if type_specs.is_empty() {
            return unsupported(
                UnsupportedTypeCategory::InvalidSpecifierCombination,
                "missing C type specifier",
            );
        }

        let special_count = type_specs
            .iter()
            .filter(|specifier| {
                matches!(
                    specifier.node,
                    TypeSpecifier::Atomic(_)
                        | TypeSpecifier::Struct(_)
                        | TypeSpecifier::Enum(_)
                        | TypeSpecifier::TypedefName(_)
                        | TypeSpecifier::TypeOf(_)
                        | TypeSpecifier::TS18661Float(_)
                        | TypeSpecifier::Int128
                        | TypeSpecifier::Float128
                        | TypeSpecifier::BitInt(_)
                )
            })
            .count();
        if special_count > 1 || (special_count == 1 && type_specs.len() > 2) {
            return unsupported(
                UnsupportedTypeCategory::InvalidSpecifierCombination,
                self.span_text(type_specs),
            );
        }

        for specifier in type_specs {
            match &specifier.node {
                TypeSpecifier::Struct(record) => {
                    return self.tag_ref(
                        record
                            .node
                            .identifier
                            .as_ref()
                            .map(|name| name.node.name.as_str()),
                        record.span,
                        true,
                    );
                }
                TypeSpecifier::Enum(enumeration) => {
                    return self.tag_ref(
                        enumeration
                            .node
                            .identifier
                            .as_ref()
                            .map(|name| name.node.name.as_str()),
                        enumeration.span,
                        false,
                    );
                }
                TypeSpecifier::TypedefName(name) => {
                    return self
                        .ordinary
                        .get(&name.node.name)
                        .copied()
                        .map(|id| supported(CTypeKind::AliasRef(id)))
                        .unwrap_or_else(|| {
                            unsupported(
                                UnsupportedTypeCategory::Other,
                                format!("unresolved typedef {}", name.node.name),
                            )
                        });
                }
                TypeSpecifier::TypeOf(_) => {
                    return unsupported(
                        UnsupportedTypeCategory::Typeof,
                        self.text(specifier.span, "typeof"),
                    );
                }
                TypeSpecifier::Atomic(type_name) => {
                    let inner_specs: Vec<_> = type_name
                        .node
                        .specifiers
                        .iter()
                        .filter_map(|specifier| match &specifier.node {
                            SpecifierQualifier::TypeSpecifier(value) => Some(value),
                            _ => None,
                        })
                        .collect();
                    let mut ty = self.base_type(&inner_specs);
                    ty.qualifiers.is_atomic = true;
                    if let Some(declarator) = &type_name.node.declarator {
                        ty = self.apply_derived(ty, &declarator.node, false);
                    }
                    return ty;
                }
                TypeSpecifier::TS18661Float(value) => {
                    let format = match value.format {
                        TS18661FloatFormat::BinaryInterchange => Ts18661Format::BinaryInterchange,
                        TS18661FloatFormat::BinaryExtended => Ts18661Format::BinaryExtended,
                        TS18661FloatFormat::DecimalInterchange => Ts18661Format::DecimalInterchange,
                        TS18661FloatFormat::DecimalExtended => Ts18661Format::DecimalExtended,
                    };
                    let Ok(width) = u16::try_from(value.width) else {
                        return unsupported(
                            UnsupportedTypeCategory::Other,
                            self.text(specifier.span, "TS 18661 floating type"),
                        );
                    };
                    return supported(CTypeKind::Floating(CFloatingType::Ts18661 {
                        format,
                        width,
                    }));
                }
                TypeSpecifier::Int128 => {
                    let signedness = if type_specs
                        .iter()
                        .any(|value| matches!(value.node, TypeSpecifier::Unsigned))
                    {
                        Signedness::Unsigned
                    } else {
                        Signedness::Signed
                    };
                    return supported(CTypeKind::Integer(CIntegerType::Int128 { signedness }));
                }
                TypeSpecifier::Float128 => {
                    return supported(CTypeKind::Floating(CFloatingType::Float128));
                }
                TypeSpecifier::BitInt(expression) => {
                    let width = match eval_const_expr(&expression.node) {
                        Some(value) if value > 0 => match u64::try_from(value) {
                            Ok(bits) => BitIntWidth::Known { bits },
                            Err(_) => {
                                return unsupported(
                                    UnsupportedTypeCategory::Other,
                                    self.text(expression.span, "_BitInt width expression"),
                                )
                            }
                        },
                        _ => BitIntWidth::Expression {
                            normalized_expression: self
                                .text(expression.span, "_BitInt width expression"),
                        },
                    };
                    let signedness = if type_specs
                        .iter()
                        .any(|value| matches!(value.node, TypeSpecifier::Unsigned))
                    {
                        Signedness::Unsigned
                    } else {
                        Signedness::Signed
                    };
                    return supported(CTypeKind::Integer(CIntegerType::BitInt {
                        signedness,
                        width,
                    }));
                }
                _ => {}
            }
        }

        self.standard_scalar(type_specs)
    }

    fn standard_scalar(&self, type_specs: &[&Node<TypeSpecifier>]) -> CType {
        let count = |needle: fn(&TypeSpecifier) -> bool| {
            type_specs
                .iter()
                .filter(|specifier| needle(&specifier.node))
                .count()
        };
        let voids = count(|value| matches!(value, TypeSpecifier::Void));
        let bools = count(|value| matches!(value, TypeSpecifier::Bool));
        let chars = count(|value| matches!(value, TypeSpecifier::Char));
        let shorts = count(|value| matches!(value, TypeSpecifier::Short));
        let ints = count(|value| matches!(value, TypeSpecifier::Int));
        let longs = count(|value| matches!(value, TypeSpecifier::Long));
        let floats = count(|value| matches!(value, TypeSpecifier::Float));
        let doubles = count(|value| matches!(value, TypeSpecifier::Double));
        let signed = count(|value| matches!(value, TypeSpecifier::Signed));
        let unsigned = count(|value| matches!(value, TypeSpecifier::Unsigned));
        let complex = count(|value| matches!(value, TypeSpecifier::Complex));

        let invalid_counts = voids > 1
            || bools > 1
            || chars > 1
            || shorts > 1
            || ints > 1
            || longs > 2
            || floats > 1
            || doubles > 1
            || signed > 1
            || unsigned > 1
            || complex > 1
            || (signed > 0 && unsigned > 0);
        if invalid_counts {
            return unsupported(
                UnsupportedTypeCategory::InvalidSpecifierCombination,
                self.span_text(type_specs),
            );
        }
        let sign = if unsigned == 1 {
            Signedness::Unsigned
        } else {
            Signedness::Signed
        };
        let total = type_specs.len();

        if voids == 1 && total == 1 {
            return supported(CTypeKind::Void);
        }
        if bools == 1 && total == 1 {
            return supported(CTypeKind::Bool);
        }
        if floats == 1 && total == 1 + complex {
            return supported(if complex == 1 {
                CTypeKind::Complex(CFloatingType::Float)
            } else {
                CTypeKind::Floating(CFloatingType::Float)
            });
        }
        if doubles == 1 && longs <= 1 && total == 1 + longs + complex {
            let value = if longs == 1 {
                CFloatingType::LongDouble
            } else {
                CFloatingType::Double
            };
            return supported(if complex == 1 {
                CTypeKind::Complex(value)
            } else {
                CTypeKind::Floating(value)
            });
        }
        if complex == 1 && total == 1 {
            return supported(CTypeKind::Complex(CFloatingType::Double));
        }
        if chars == 1 && total == 1 + signed + unsigned {
            let signedness = if unsigned == 1 {
                CharTypeSignedness::Unsigned
            } else if signed == 1 {
                CharTypeSignedness::Signed
            } else {
                CharTypeSignedness::Plain
            };
            return supported(CTypeKind::Integer(CIntegerType::Char { signedness }));
        }
        if shorts == 1 && longs == 0 && total == shorts + ints + signed + unsigned && ints <= 1 {
            return supported(CTypeKind::Integer(CIntegerType::Short { signedness: sign }));
        }
        if longs == 1 && shorts == 0 && total == longs + ints + signed + unsigned && ints <= 1 {
            return supported(CTypeKind::Integer(CIntegerType::Long { signedness: sign }));
        }
        if longs == 2 && shorts == 0 && total == longs + ints + signed + unsigned && ints <= 1 {
            return supported(CTypeKind::Integer(CIntegerType::LongLong {
                signedness: sign,
            }));
        }
        if total == ints + signed + unsigned && ints <= 1 && ints + signed + unsigned > 0 {
            return supported(CTypeKind::Integer(CIntegerType::Int { signedness: sign }));
        }
        unsupported(
            UnsupportedTypeCategory::InvalidSpecifierCombination,
            self.span_text(type_specs),
        )
    }

    fn tag_ref(&self, name: Option<&str>, span: Span, record: bool) -> CType {
        let id = match name {
            Some(name) => self.tags.get(name).copied(),
            None => self.anonymous_tags.get(&(span.start, span.end)).copied(),
        };
        match id {
            Some(id) if record => supported(CTypeKind::RecordRef(id)),
            Some(id) => supported(CTypeKind::EnumRef(id)),
            None => unsupported(
                UnsupportedTypeCategory::Other,
                name.map_or_else(
                    || "unresolved anonymous tag".to_owned(),
                    |name| format!("unresolved tag {name}"),
                ),
            ),
        }
    }

    fn span_text(&self, specifiers: &[&Node<TypeSpecifier>]) -> String {
        let start = specifiers.iter().map(|value| value.span.start).min();
        let end = specifiers.iter().map(|value| value.span.end).max();
        match (start, end) {
            (Some(start), Some(end)) => {
                self.text(Span { start, end }, "invalid C type specifier span")
            }
            _ => "invalid C type".to_owned(),
        }
    }

    pub(crate) fn text(&self, span: Span, semantic_fallback: &str) -> String {
        if span.is_none() || span.start > span.end || span.end > self.source.len() {
            return semantic_fallback.to_owned();
        }
        self.source[span.start..span.end].trim().to_owned()
    }
}

pub(crate) fn declarator_name(declarator: &Declarator) -> Option<String> {
    match &declarator.kind.node {
        DeclaratorKind::Identifier(identifier) => Some(identifier.node.name.clone()),
        DeclaratorKind::Declarator(inner) => declarator_name(&inner.node),
        DeclaratorKind::Abstract => None,
    }
}

pub(crate) fn declarator_name_span(declarator: &Declarator) -> Option<Span> {
    match &declarator.kind.node {
        DeclaratorKind::Identifier(identifier) => Some(identifier.span),
        DeclaratorKind::Declarator(inner) => declarator_name_span(&inner.node),
        DeclaratorKind::Abstract => None,
    }
}

pub(crate) fn is_function_declarator(declarator: &Declarator) -> bool {
    declarator
        .derived
        .iter()
        .any(|derived| matches!(derived.node, DerivedDeclarator::Function(_)))
}

fn declaration_qualifiers(
    specifiers: &[Node<DeclarationSpecifier>],
) -> (TypeQualifiers, Nullability) {
    qualifier_values(
        specifiers
            .iter()
            .filter_map(|specifier| match &specifier.node {
                DeclarationSpecifier::TypeQualifier(value) => Some(&value.node),
                _ => None,
            }),
    )
}

fn field_qualifiers(specifiers: &[Node<SpecifierQualifier>]) -> (TypeQualifiers, Nullability) {
    qualifier_values(
        specifiers
            .iter()
            .filter_map(|specifier| match &specifier.node {
                SpecifierQualifier::TypeQualifier(value) => Some(&value.node),
                _ => None,
            }),
    )
}

fn pointer_qualifiers(
    qualifiers: &[Node<PointerQualifier>],
) -> (TypeQualifiers, Nullability, bool) {
    let values = qualifier_values(qualifiers.iter().filter_map(
        |qualifier| match &qualifier.node {
            PointerQualifier::TypeQualifier(value) => Some(&value.node),
            PointerQualifier::Extension(_) => None,
        },
    ));
    let has_extensions = qualifiers
        .iter()
        .any(|qualifier| matches!(qualifier.node, PointerQualifier::Extension(_)));
    (values.0, values.1, has_extensions)
}

fn qualifier_values<'a>(
    qualifiers: impl Iterator<Item = &'a TypeQualifier>,
) -> (TypeQualifiers, Nullability) {
    let mut result = TypeQualifiers::NONE;
    let mut nullability = Nullability::Unspecified;
    for qualifier in qualifiers {
        match qualifier {
            TypeQualifier::Const => result.is_const = true,
            TypeQualifier::Volatile => result.is_volatile = true,
            TypeQualifier::Restrict => result.is_restrict = true,
            TypeQualifier::Atomic => result.is_atomic = true,
            TypeQualifier::Nonnull => nullability = Nullability::Nonnull,
            TypeQualifier::Nullable => nullability = Nullability::Nullable,
            TypeQualifier::NullUnspecified => nullability = Nullability::NullUnspecified,
        }
    }
    (result, nullability)
}

fn pointer(inner: CType, values: (TypeQualifiers, Nullability, bool)) -> CType {
    let (qualifiers, nullability, has_extensions) = values;
    CType {
        qualifiers,
        nullability,
        kind: CTypeKind::Pointer(Box::new(inner)),
        support: if has_extensions {
            partial(
                "PARC-P1102",
                "pointer extension qualifiers require explicit downstream review",
            )
        } else {
            SupportStatus::Supported
        },
    }
}

fn array_type(
    source: &str,
    element: CType,
    size: &ArraySize,
    array_qualifiers: &[Node<TypeQualifier>],
    parameter_context: bool,
) -> CType {
    let (written_qualifiers, nullability) =
        qualifier_values(array_qualifiers.iter().map(|qualifier| &qualifier.node));
    let parameter_qualifiers = if parameter_context {
        written_qualifiers
    } else {
        TypeQualifiers::NONE
    };
    let (bound, support) = match size {
        ArraySize::Unknown => (ArrayBound::Incomplete, SupportStatus::Supported),
        ArraySize::VariableUnknown => (
            ArrayBound::Variable {
                normalized_expression: "*".to_owned(),
            },
            SupportStatus::Supported,
        ),
        ArraySize::VariableExpression(expression) => {
            let spelling = span_text(source, expression.span, "array bound expression");
            let bound = match eval_const_expr(&expression.node) {
                Some(value) if value > 0 => match u64::try_from(value) {
                    Ok(elements) => ArrayBound::Fixed { elements },
                    Err(_) => ArrayBound::Invalid {
                        spelling,
                        diagnostic: code("PARC-E1103"),
                    },
                },
                Some(_) => ArrayBound::Invalid {
                    spelling,
                    diagnostic: code("PARC-E1103"),
                },
                None => ArrayBound::Variable {
                    normalized_expression: spelling,
                },
            };
            (bound, SupportStatus::Supported)
        }
        ArraySize::StaticExpression(expression) if parameter_context => {
            let spelling = span_text(source, expression.span, "array bound expression");
            let minimum = match eval_const_expr(&expression.node) {
                Some(value) if value > 0 => match u64::try_from(value) {
                    Ok(elements) => ArrayMinimumBound::Fixed { elements },
                    Err(_) => ArrayMinimumBound::Variable {
                        normalized_expression: spelling,
                    },
                },
                _ => ArrayMinimumBound::Variable {
                    normalized_expression: spelling,
                },
            };
            (
                ArrayBound::StaticMinimum { minimum },
                SupportStatus::Supported,
            )
        }
        ArraySize::StaticExpression(expression) => (
            ArrayBound::Invalid {
                spelling: span_text(source, expression.span, "static array bound"),
                diagnostic: code("PARC-E1105"),
            },
            SupportStatus::Unsupported {
                code: code("PARC-E1105"),
                reason: "static minimum array bound is only valid on a parameter".to_owned(),
            },
        ),
    };
    let support = if !parameter_context && !array_qualifiers.is_empty() {
        SupportStatus::Unsupported {
            code: code("PARC-E1106"),
            reason: "array-bracket qualifiers are only valid on a parameter".to_owned(),
        }
    } else if nullability == Nullability::Unspecified {
        support
    } else {
        partial(
            "PARC-P1101",
            "array-bracket nullability cannot be represented by the source contract",
        )
    };
    CType {
        qualifiers: TypeQualifiers::NONE,
        nullability: Nullability::Unspecified,
        kind: CTypeKind::Array {
            element: Box::new(element),
            bound,
            parameter_qualifiers,
        },
        support,
    }
}

fn declarator_calling_convention(declarator: &Declarator) -> CallingConvention {
    fn visit(
        declarator: &Declarator,
        found: &mut Option<CallingConvention>,
        spellings: &mut Vec<String>,
    ) -> bool {
        for extension in &declarator.extensions {
            if let Extension::Attribute(attribute) = &extension.node {
                match super::modeled_calling_convention(attribute) {
                    Ok(Some(convention)) => {
                        spellings.push(attribute.name.node.clone());
                        if found
                            .as_ref()
                            .is_some_and(|existing| existing != &convention)
                        {
                            return false;
                        }
                        *found = Some(convention);
                    }
                    Ok(None) => {}
                    Err(()) => {
                        spellings.push(attribute.name.node.clone());
                        return false;
                    }
                }
            }
        }
        if let DeclaratorKind::Declarator(inner) = &declarator.kind.node {
            return visit(&inner.node, found, spellings);
        }
        true
    }

    let mut found = None;
    let mut spellings = Vec::new();
    if !visit(declarator, &mut found, &mut spellings) {
        return CallingConvention::Unsupported {
            spelling: spellings.join(" "),
        };
    }
    found.unwrap_or(CallingConvention::C)
}

fn span_text(source: &str, span: Span, semantic_fallback: &str) -> String {
    if span.is_none() || span.start > span.end || span.end > source.len() {
        semantic_fallback.to_owned()
    } else {
        source[span.start..span.end].trim().to_owned()
    }
}

fn supported(kind: CTypeKind) -> CType {
    CType {
        qualifiers: TypeQualifiers::NONE,
        nullability: Nullability::Unspecified,
        kind,
        support: SupportStatus::Supported,
    }
}

fn unsupported(category: UnsupportedTypeCategory, spelling: impl Into<String>) -> CType {
    let spelling = spelling.into();
    CType {
        qualifiers: TypeQualifiers::NONE,
        nullability: Nullability::Unspecified,
        kind: CTypeKind::Unsupported {
            category,
            spelling: spelling.clone(),
        },
        support: SupportStatus::Unsupported {
            code: code("PARC-E1100"),
            reason: format!("unsupported C type: {spelling}"),
        },
    }
}

fn partial(code_value: &str, reason: &str) -> SupportStatus {
    SupportStatus::Partial {
        code: code(code_value),
        reason: reason.to_owned(),
    }
}

pub(crate) fn code(value: &str) -> DiagnosticCode {
    DiagnosticCode::new(value).expect("static diagnostic code")
}

/// Best-effort evaluation that never substitutes a value when evaluation fails.
pub(crate) fn eval_const_expr(expr: &Expression) -> Option<i128> {
    match expr {
        Expression::Constant(constant) => match &constant.node {
            Constant::Integer(integer) => {
                let number = integer.number.as_ref();
                match integer.base {
                    IntegerBase::Decimal => number.parse::<i128>().ok(),
                    IntegerBase::Octal => i128::from_str_radix(number, 8).ok(),
                    IntegerBase::Hexadecimal => i128::from_str_radix(number, 16).ok(),
                    IntegerBase::Binary => i128::from_str_radix(number, 2).ok(),
                }
            }
            _ => None,
        },
        Expression::UnaryOperator(unary) => {
            let inner = eval_const_expr(&unary.node.operand.node)?;
            match unary.node.operator.node {
                UnaryOperator::Minus => inner.checked_neg(),
                UnaryOperator::Plus => Some(inner),
                UnaryOperator::Complement => Some(!inner),
                UnaryOperator::Negate => Some(i128::from(inner == 0)),
                _ => None,
            }
        }
        Expression::BinaryOperator(binary) => {
            let lhs = eval_const_expr(&binary.node.lhs.node)?;
            let rhs = eval_const_expr(&binary.node.rhs.node)?;
            match binary.node.operator.node {
                BinaryOperator::Plus => lhs.checked_add(rhs),
                BinaryOperator::Minus => lhs.checked_sub(rhs),
                BinaryOperator::Multiply => lhs.checked_mul(rhs),
                BinaryOperator::Divide if rhs != 0 => lhs.checked_div(rhs),
                BinaryOperator::Modulo if rhs != 0 => lhs.checked_rem(rhs),
                BinaryOperator::ShiftLeft => u32::try_from(rhs)
                    .ok()
                    .and_then(|shift| lhs.checked_shl(shift)),
                BinaryOperator::ShiftRight => u32::try_from(rhs)
                    .ok()
                    .and_then(|shift| lhs.checked_shr(shift)),
                BinaryOperator::BitwiseAnd => Some(lhs & rhs),
                BinaryOperator::BitwiseOr => Some(lhs | rhs),
                BinaryOperator::BitwiseXor => Some(lhs ^ rhs),
                BinaryOperator::Equals => Some(i128::from(lhs == rhs)),
                BinaryOperator::NotEquals => Some(i128::from(lhs != rhs)),
                BinaryOperator::Less => Some(i128::from(lhs < rhs)),
                BinaryOperator::Greater => Some(i128::from(lhs > rhs)),
                BinaryOperator::LessOrEqual => Some(i128::from(lhs <= rhs)),
                BinaryOperator::GreaterOrEqual => Some(i128::from(lhs >= rhs)),
                BinaryOperator::LogicalAnd => Some(i128::from(lhs != 0 && rhs != 0)),
                BinaryOperator::LogicalOr => Some(i128::from(lhs != 0 || rhs != 0)),
                _ => None,
            }
        }
        Expression::Conditional(conditional) => {
            if eval_const_expr(&conditional.node.condition.node)? != 0 {
                eval_const_expr(&conditional.node.then_expression.node)
            } else {
                eval_const_expr(&conditional.node.else_expression.node)
            }
        }
        Expression::Cast(cast) => eval_const_expr(&cast.node.expression.node),
        Expression::Comma(parts) => parts.last().and_then(|part| eval_const_expr(&part.node)),
        _ => None,
    }
}

/// Exact evaluation for the subset whose signedness is explicit without a
/// target-dependent integer-conversion proof. Other expressions remain
/// unevaluated rather than being narrowed through `i128`.
pub(crate) fn eval_exact_integer(expr: &Expression) -> Option<ExactInteger> {
    match expr {
        Expression::Constant(constant) => match &constant.node {
            Constant::Integer(integer) if !integer.suffix.imaginary => {
                let radix = match integer.base {
                    IntegerBase::Decimal => 10,
                    IntegerBase::Octal => 8,
                    IntegerBase::Hexadecimal => 16,
                    IntegerBase::Binary => 2,
                };
                let magnitude = u128::from_str_radix(integer.number.as_ref(), radix).ok()?;
                if integer.suffix.unsigned {
                    Some(ExactInteger::unsigned(magnitude))
                } else {
                    i128::try_from(magnitude).ok().map(ExactInteger::signed)
                }
            }
            _ => None,
        },
        Expression::UnaryOperator(unary) => {
            let value = eval_exact_integer(&unary.node.operand.node)?;
            match unary.node.operator.node {
                UnaryOperator::Plus => Some(value),
                UnaryOperator::Minus => value
                    .as_signed()
                    .and_then(i128::checked_neg)
                    .map(ExactInteger::signed),
                _ => None,
            }
        }
        _ => None,
    }
}
