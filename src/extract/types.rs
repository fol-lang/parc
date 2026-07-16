//! Type resolution and constant expression evaluation for the extractor.

use crate::ast::*;
use crate::ir::{SourceType, TypeQualifiers};
use crate::span::Node;

/// Apply a type qualifier to a mutable qualifiers struct.
pub fn apply_type_qualifier(qualifiers: &mut TypeQualifiers, qualifier: &TypeQualifier) {
    match qualifier {
        TypeQualifier::Const => qualifiers.is_const = true,
        TypeQualifier::Volatile => qualifiers.is_volatile = true,
        TypeQualifier::Restrict => qualifiers.is_restrict = true,
        TypeQualifier::Atomic => qualifiers.is_atomic = true,
        TypeQualifier::Nonnull | TypeQualifier::NullUnspecified | TypeQualifier::Nullable => {}
    }
}

/// Resolve the base type from declaration specifiers.
pub fn resolve_base_type(specifiers: &[Node<DeclarationSpecifier>]) -> SourceType {
    let type_specs: Vec<_> = specifiers
        .iter()
        .filter_map(|s| match &s.node {
            DeclarationSpecifier::TypeSpecifier(ts) => Some(ts),
            _ => None,
        })
        .collect();
    resolve_base_type_from_type_specs(&type_specs)
}

/// Resolve the base type from parameter specifiers.
pub fn resolve_base_type_from_param_specifiers(
    specifiers: &[Node<DeclarationSpecifier>],
) -> SourceType {
    let type_specs: Vec<_> = specifiers
        .iter()
        .filter_map(|s| match &s.node {
            DeclarationSpecifier::TypeSpecifier(ts) => Some(ts),
            _ => None,
        })
        .collect();
    resolve_base_type_from_type_specs(&type_specs)
}

/// Resolve a base type from a list of type specifier nodes.
pub fn resolve_base_type_from_type_specs(type_specs: &[&Node<TypeSpecifier>]) -> SourceType {
    if type_specs.is_empty() {
        return SourceType::Int; // C default
    }

    let mut has_void = false;
    let mut has_char = false;
    let mut has_short = false;
    let mut has_int = false;
    let mut long_count = 0u8;
    let mut has_float = false;
    let mut has_double = false;
    let mut has_signed = false;
    let mut has_unsigned = false;
    let mut has_bool = false;

    for ts in type_specs {
        match &ts.node {
            TypeSpecifier::Void => has_void = true,
            TypeSpecifier::Char => has_char = true,
            TypeSpecifier::Short => has_short = true,
            TypeSpecifier::Int => has_int = true,
            TypeSpecifier::Long => long_count += 1,
            TypeSpecifier::Float => has_float = true,
            TypeSpecifier::Double => has_double = true,
            TypeSpecifier::Signed => has_signed = true,
            TypeSpecifier::Unsigned => has_unsigned = true,
            TypeSpecifier::Bool => has_bool = true,
            TypeSpecifier::Struct(st) => {
                let name = st
                    .node
                    .identifier
                    .as_ref()
                    .map(|id| id.node.name.clone())
                    .unwrap_or_else(|| "<anonymous>".into());
                return SourceType::RecordRef(name);
            }
            TypeSpecifier::Enum(et) => {
                let name = et
                    .node
                    .identifier
                    .as_ref()
                    .map(|id| id.node.name.clone())
                    .unwrap_or_else(|| "<anonymous>".into());
                return SourceType::EnumRef(name);
            }
            TypeSpecifier::TypedefName(id) => {
                return SourceType::TypedefRef(id.node.name.clone());
            }
            TypeSpecifier::Complex => {
                return SourceType::Opaque("_Complex".into());
            }
            TypeSpecifier::TypeOf(_) => {
                return SourceType::Opaque("typeof".into());
            }
            _ => {}
        }
    }

    if has_void {
        return SourceType::Void;
    }
    if has_bool {
        return SourceType::Bool;
    }
    if has_float {
        return SourceType::Float;
    }
    if has_double && long_count > 0 {
        return SourceType::LongDouble;
    }
    if has_double {
        return SourceType::Double;
    }
    if has_char {
        return if has_unsigned {
            SourceType::UChar
        } else if has_signed {
            SourceType::SChar
        } else {
            SourceType::Char
        };
    }
    if has_short {
        return if has_unsigned {
            SourceType::UShort
        } else {
            SourceType::Short
        };
    }
    if long_count >= 2 {
        return if has_unsigned {
            SourceType::ULongLong
        } else {
            SourceType::LongLong
        };
    }
    if long_count == 1 {
        return if has_unsigned {
            SourceType::ULong
        } else {
            SourceType::Long
        };
    }
    if has_unsigned {
        return SourceType::UInt;
    }
    if has_signed || has_int {
        return SourceType::Int;
    }

    SourceType::Int
}

/// Extract qualifiers from declaration specifiers.
pub fn qualifiers_from_declaration_specifiers(
    specifiers: &[Node<DeclarationSpecifier>],
) -> TypeQualifiers {
    let mut qualifiers = TypeQualifiers::default();
    for specifier in specifiers {
        if let DeclarationSpecifier::TypeQualifier(type_qualifier) = &specifier.node {
            apply_type_qualifier(&mut qualifiers, &type_qualifier.node);
        }
    }
    qualifiers
}

/// Extract qualifiers from specifier-qualifier list (struct fields).
pub fn qualifiers_from_specifier_qualifiers(
    specifiers: &[Node<SpecifierQualifier>],
) -> TypeQualifiers {
    let mut qualifiers = TypeQualifiers::default();
    for specifier in specifiers {
        if let SpecifierQualifier::TypeQualifier(type_qualifier) = &specifier.node {
            apply_type_qualifier(&mut qualifiers, &type_qualifier.node);
        }
    }
    qualifiers
}

/// Extract qualifiers from pointer qualifiers.
pub fn qualifiers_from_pointer_qualifiers(qualifiers: &[Node<PointerQualifier>]) -> TypeQualifiers {
    let mut resolved = TypeQualifiers::default();
    for qualifier in qualifiers {
        if let PointerQualifier::TypeQualifier(type_qualifier) = &qualifier.node {
            apply_type_qualifier(&mut resolved, &type_qualifier.node);
        }
    }
    resolved
}

/// Apply derived declarator types (pointers, arrays, function pointers) to a base type.
pub fn apply_derived_type(base: SourceType, declarator: &Declarator) -> SourceType {
    let mut ty = base;
    let mut pointers: Vec<TypeQualifiers> = Vec::new();

    for derived in &declarator.derived {
        match &derived.node {
            DerivedDeclarator::Pointer(quals) => {
                pointers.push(qualifiers_from_pointer_qualifiers(quals));
            }
            DerivedDeclarator::Array(arr) => {
                let size = match &arr.node.size {
                    ArraySize::Unknown | ArraySize::VariableUnknown => None,
                    ArraySize::VariableExpression(expr) | ArraySize::StaticExpression(expr) => {
                        eval_const_expr(&expr.node).map(|v| v as u64)
                    }
                };
                ty = SourceType::Array(Box::new(ty), size);
            }
            DerivedDeclarator::Function(fdecl) => {
                let params = extract_parameter_types(&fdecl.node.parameters);
                let variadic = fdecl.node.ellipsis == Ellipsis::Some;
                ty = SourceType::FunctionPointer {
                    return_type: Box::new(ty),
                    parameters: params,
                    variadic,
                };
            }
            DerivedDeclarator::KRFunction(_) | DerivedDeclarator::Block(_) => {}
        }
    }

    // Pointers wrap inside-out. Pointer qualifier `const` means the pointer itself
    // is const, not that the pointee is const. Pointee constness comes from the base
    // specifiers and is handled by mark_innermost_pointer_const.
    for pointer_qualifiers in &pointers {
        ty = SourceType::Pointer {
            pointee: Box::new(ty),
            qualifiers: *pointer_qualifiers,
        };
    }

    // Handle nested declarator (parenthesized)
    if let DeclaratorKind::Declarator(inner) = &declarator.kind.node {
        ty = apply_derived_type(ty, &inner.node);
    }

    ty
}

/// Resolve the full type from specifiers and declarator.
pub fn resolve_full_type(
    specifiers: &[Node<DeclarationSpecifier>],
    declarator: &Declarator,
) -> SourceType {
    let base = resolve_base_type(specifiers);
    let base_qualifiers = qualifiers_from_declaration_specifiers(specifiers);
    let mut ty = apply_derived_type(base, declarator);

    if base_qualifiers.is_const {
        ty = mark_innermost_pointer_const(ty);
    }
    apply_base_qualifiers(ty, base_qualifiers)
}

/// Mark the innermost pointer in a type as const-pointee.
pub fn mark_innermost_pointer_const(ty: SourceType) -> SourceType {
    match ty {
        SourceType::Pointer {
            pointee,
            qualifiers,
        } => match *pointee {
            inner @ SourceType::Pointer { .. } => SourceType::Pointer {
                pointee: Box::new(mark_innermost_pointer_const(inner)),
                qualifiers,
            },
            other => SourceType::Pointer {
                pointee: Box::new(other),
                qualifiers: TypeQualifiers {
                    is_const: true,
                    ..qualifiers
                },
            },
        },
        SourceType::Qualified { ty, qualifiers } => SourceType::Qualified {
            ty: Box::new(mark_innermost_pointer_const(*ty)),
            qualifiers,
        },
        other => other,
    }
}

/// Apply base qualifiers, suppressing const if a pointer layer is present
/// (const was already handled by mark_innermost_pointer_const).
pub fn apply_base_qualifiers(ty: SourceType, mut qualifiers: TypeQualifiers) -> SourceType {
    if qualifiers.is_const && type_has_pointer_layer(&ty) {
        qualifiers.is_const = false;
    }
    SourceType::qualified(ty, qualifiers)
}

/// Check if a type has a pointer at the outermost level.
pub fn type_has_pointer_layer(ty: &SourceType) -> bool {
    match ty {
        SourceType::Pointer { .. } => true,
        SourceType::Qualified { ty, .. } => type_has_pointer_layer(ty),
        _ => false,
    }
}

/// Extract parameter types (just the types, not names) for function pointer construction.
fn extract_parameter_types(params: &[Node<ParameterDeclaration>]) -> Vec<SourceType> {
    // Handle `void` parameter
    if params.len() == 1 {
        let p = &params[0].node;
        if p.declarator.is_none() {
            let base = resolve_base_type_from_param_specifiers(&p.specifiers);
            if base == SourceType::Void {
                return Vec::new();
            }
        }
    }

    params
        .iter()
        .map(|p| {
            let base = resolve_base_type_from_param_specifiers(&p.node.specifiers);
            let base_qualifiers = qualifiers_from_declaration_specifiers(&p.node.specifiers);
            let mut ty = match &p.node.declarator {
                Some(d) => apply_derived_type(base, &d.node),
                None => base,
            };
            if base_qualifiers.is_const {
                ty = mark_innermost_pointer_const(ty);
            }
            apply_base_qualifiers(ty, base_qualifiers)
        })
        .collect()
}

/// Best-effort constant expression evaluation for enum values and array sizes.
pub fn eval_const_expr(expr: &Expression) -> Option<i128> {
    match expr {
        Expression::Constant(c) => match &c.node {
            Constant::Integer(i) => {
                let s = i.number.as_ref();
                match i.base {
                    IntegerBase::Decimal => s.parse::<i128>().ok(),
                    IntegerBase::Octal => i128::from_str_radix(s, 8).ok(),
                    IntegerBase::Hexadecimal => i128::from_str_radix(s, 16).ok(),
                    IntegerBase::Binary => i128::from_str_radix(s, 2).ok(),
                }
            }
            _ => None,
        },
        Expression::UnaryOperator(u) => {
            let inner = eval_const_expr(&u.node.operand.node)?;
            match u.node.operator.node {
                UnaryOperator::Minus => Some(-inner),
                UnaryOperator::Plus => Some(inner),
                UnaryOperator::Complement => Some(!inner),
                UnaryOperator::Negate => Some(if inner == 0 { 1 } else { 0 }),
                _ => None,
            }
        }
        Expression::BinaryOperator(b) => {
            let lhs = eval_const_expr(&b.node.lhs.node)?;
            let rhs = eval_const_expr(&b.node.rhs.node)?;
            match b.node.operator.node {
                BinaryOperator::Plus => Some(lhs + rhs),
                BinaryOperator::Minus => Some(lhs - rhs),
                BinaryOperator::Multiply => Some(lhs * rhs),
                BinaryOperator::Divide if rhs != 0 => Some(lhs / rhs),
                BinaryOperator::Modulo if rhs != 0 => Some(lhs % rhs),
                BinaryOperator::ShiftLeft => Some(lhs << (rhs as u32)),
                BinaryOperator::ShiftRight => Some(lhs >> (rhs as u32)),
                BinaryOperator::BitwiseAnd => Some(lhs & rhs),
                BinaryOperator::BitwiseOr => Some(lhs | rhs),
                BinaryOperator::BitwiseXor => Some(lhs ^ rhs),
                BinaryOperator::Equals => Some(if lhs == rhs { 1 } else { 0 }),
                BinaryOperator::NotEquals => Some(if lhs != rhs { 1 } else { 0 }),
                BinaryOperator::Less => Some(if lhs < rhs { 1 } else { 0 }),
                BinaryOperator::Greater => Some(if lhs > rhs { 1 } else { 0 }),
                BinaryOperator::LessOrEqual => Some(if lhs <= rhs { 1 } else { 0 }),
                BinaryOperator::GreaterOrEqual => Some(if lhs >= rhs { 1 } else { 0 }),
                BinaryOperator::LogicalAnd => Some(if lhs != 0 && rhs != 0 { 1 } else { 0 }),
                BinaryOperator::LogicalOr => Some(if lhs != 0 || rhs != 0 { 1 } else { 0 }),
                _ => None,
            }
        }
        Expression::Conditional(c) => {
            let cond = eval_const_expr(&c.node.condition.node)?;
            if cond != 0 {
                eval_const_expr(&c.node.then_expression.node)
            } else {
                eval_const_expr(&c.node.else_expression.node)
            }
        }
        Expression::Cast(c) => eval_const_expr(&c.node.expression.node),
        Expression::Comma(parts) => parts.last().and_then(|e| eval_const_expr(&e.node)),
        _ => None,
    }
}
