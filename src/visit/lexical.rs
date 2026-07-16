use crate::ast::*;
use crate::span::Span;

use super::Visit;

pub fn visit_identifier<'ast, V: Visit<'ast> + ?Sized>(
    _visitor: &mut V,
    _identifier: &'ast Identifier,
    _span: &'ast Span,
) {
}

pub fn visit_constant<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    constant: &'ast Constant,
    span: &'ast Span,
) {
    match *constant {
        Constant::Integer(ref i) => visitor.visit_integer(i, span),
        Constant::Float(ref f) => visitor.visit_float(f, span),
        Constant::Character(_) => {}
    }
}

pub fn visit_integer<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    integer: &'ast Integer,
    span: &'ast Span,
) {
    visitor.visit_integer_base(&integer.base, span);
    visitor.visit_integer_suffix(&integer.suffix, span);
}

pub fn visit_integer_base<'ast, V: Visit<'ast> + ?Sized>(
    _visitor: &mut V,
    _integer_base: &'ast IntegerBase,
    _span: &'ast Span,
) {
}

pub fn visit_integer_suffix<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    integer_suffix: &'ast IntegerSuffix,
    span: &'ast Span,
) {
    visitor.visit_integer_size(&integer_suffix.size, span);
}

pub fn visit_integer_size<'ast, V: Visit<'ast> + ?Sized>(
    _visitor: &mut V,
    _integer_size: &'ast IntegerSize,
    _span: &'ast Span,
) {
}

pub fn visit_float<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    float: &'ast Float,
    span: &'ast Span,
) {
    visitor.visit_float_base(&float.base, span);
    visitor.visit_float_suffix(&float.suffix, span);
}

pub fn visit_float_base<'ast, V: Visit<'ast> + ?Sized>(
    _visitor: &mut V,
    _float_base: &'ast FloatBase,
    _span: &'ast Span,
) {
}

pub fn visit_float_suffix<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    float_suffix: &'ast FloatSuffix,
    span: &'ast Span,
) {
    visitor.visit_float_format(&float_suffix.format, span);
}

pub fn visit_float_format<'ast, V: Visit<'ast> + ?Sized>(
    visitor: &mut V,
    float_format: &'ast FloatFormat,
    span: &'ast Span,
) {
    if let FloatFormat::TS18661Format(ref f) = *float_format {
        visitor.visit_ts18661_float_type(f, span)
    }
}

pub fn visit_string_literal<'ast, V: Visit<'ast> + ?Sized>(
    _visitor: &mut V,
    _string_literal: &'ast StringLiteral,
    _span: &'ast Span,
) {
}
