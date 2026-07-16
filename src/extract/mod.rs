//! Source-level declaration extraction from a parsed C AST.
//!
//! This module converts parser AST nodes into the normalized PARC source IR.
//! It handles typedefs, functions, records, enums, variables, and unsupported
//! declarations, producing diagnostics for partial or unrepresentable constructs.

mod types;

use crate::ast::*;
use crate::ir::{
    CallingConvention, DiagnosticKind, RecordKind, SourceDiagnostic, SourceEnum, SourceEnumVariant,
    SourceField, SourceFunction, SourceItem, SourcePackage, SourceParameter, SourceRecord,
    SourceType, SourceTypeAlias, SourceVariable, TypeQualifiers,
};
use crate::span::Node;

use types::*;

/// The extractor walks a translation unit and produces source items + diagnostics.
pub struct Extractor {
    items: Vec<SourceItem>,
    diagnostics: Vec<SourceDiagnostic>,
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Walk all external declarations and produce items + diagnostics.
    pub fn extract(mut self, unit: &TranslationUnit) -> (Vec<SourceItem>, Vec<SourceDiagnostic>) {
        for ext_decl in &unit.0 {
            self.extract_external_declaration(ext_decl);
        }
        (self.items, self.diagnostics)
    }

    fn extract_external_declaration(&mut self, ext_decl: &Node<ExternalDeclaration>) {
        match &ext_decl.node {
            ExternalDeclaration::Declaration(decl) => {
                self.extract_declaration(&decl.node, decl.span.start);
            }
            ExternalDeclaration::FunctionDefinition(fdef) => {
                self.extract_function_definition(&fdef.node, fdef.span.start);
            }
            ExternalDeclaration::StaticAssert(_) => {
                self.diagnostics.push(
                    SourceDiagnostic::warning(
                        DiagnosticKind::DeclarationPartial,
                        "_Static_assert ignored",
                    )
                    .with_location(None, ext_decl.span.start),
                );
            }
        }
    }

    fn extract_declaration(&mut self, decl: &Declaration, offset: usize) {
        let is_typedef = decl.specifiers.iter().any(|s| {
            matches!(
                s.node,
                DeclarationSpecifier::StorageClass(ref sc)
                    if sc.node == StorageClassSpecifier::Typedef
            )
        });

        let is_extern = decl.specifiers.iter().any(|s| {
            matches!(
                s.node,
                DeclarationSpecifier::StorageClass(ref sc)
                    if sc.node == StorageClassSpecifier::Extern
            )
        });

        // Standalone struct/union/enum without declarators
        if decl.declarators.is_empty() {
            self.extract_standalone_type_specifiers(&decl.specifiers, offset);
            return;
        }

        for init_decl in &decl.declarators {
            let declarator = &init_decl.node.declarator;

            if is_typedef {
                self.extract_typedef(&decl.specifiers, &declarator.node, offset);
            } else if is_function_declarator(&declarator.node) {
                self.extract_function_declaration(&decl.specifiers, &declarator.node, offset);
            } else {
                self.extract_variable_or_type(
                    &decl.specifiers,
                    &declarator.node,
                    is_extern,
                    offset,
                );
            }
        }
    }

    fn extract_function_definition(&mut self, fdef: &FunctionDefinition, offset: usize) {
        let name = match declarator_name(&fdef.declarator.node) {
            Some(n) => n,
            None => return,
        };

        let is_static = fdef.specifiers.iter().any(|s| {
            matches!(
                s.node,
                DeclarationSpecifier::StorageClass(ref sc)
                    if sc.node == StorageClassSpecifier::Static
            )
        });
        if is_static {
            self.diagnostics.push(
                SourceDiagnostic::warning(
                    DiagnosticKind::DeclarationUnsupported,
                    format!("static function '{}' not bindable", name),
                )
                .with_item(&name)
                .with_location(None, offset),
            );
            return;
        }

        let calling_convention = detect_calling_convention(&fdef.declarator.node.extensions)
            .unwrap_or(CallingConvention::C);
        self.emit_extension_diagnostics(&fdef.declarator.node.extensions, &name, offset);
        self.emit_specifier_diagnostics(&fdef.specifiers, &name, offset);
        self.emit_derived_diagnostics(&fdef.declarator.node, &name, offset);

        let base_ty = resolve_base_type(&fdef.specifiers);
        let base_qualifiers = qualifiers_from_declaration_specifiers(&fdef.specifiers);
        let (mut return_type, params, variadic) =
            resolve_function_parts(&fdef.declarator.node, base_ty);
        if base_qualifiers.is_const {
            return_type = mark_innermost_pointer_const(return_type);
        }
        return_type = apply_base_qualifiers(return_type, base_qualifiers);

        self.items.push(SourceItem::Function(SourceFunction {
            name,
            calling_convention,
            parameters: params,
            return_type,
            variadic,
            source_offset: Some(offset),
        }));
    }

    fn extract_standalone_type_specifiers(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        offset: usize,
    ) {
        for spec in specifiers {
            if let DeclarationSpecifier::TypeSpecifier(ts) = &spec.node {
                match &ts.node {
                    TypeSpecifier::Struct(st) => {
                        self.extract_record(&st.node, offset);
                    }
                    TypeSpecifier::Enum(et) => {
                        self.extract_enum(&et.node, offset);
                    }
                    _ => {}
                }
            }
        }
    }

    fn extract_typedef(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: &Declarator,
        offset: usize,
    ) {
        let name = match declarator_name(declarator) {
            Some(n) => n,
            None => return,
        };

        self.extract_inline_type_definitions(specifiers, offset);

        let target = resolve_full_type(specifiers, declarator);
        self.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name,
            target,
            source_offset: Some(offset),
        }));
    }

    fn extract_function_declaration(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: &Declarator,
        offset: usize,
    ) {
        let name = match declarator_name(declarator) {
            Some(n) => n,
            None => return,
        };

        let calling_convention =
            detect_calling_convention(&declarator.extensions).unwrap_or(CallingConvention::C);
        self.emit_extension_diagnostics(&declarator.extensions, &name, offset);
        self.emit_specifier_diagnostics(specifiers, &name, offset);
        self.emit_derived_diagnostics(declarator, &name, offset);

        let base_ty = resolve_base_type(specifiers);
        let base_qualifiers = qualifiers_from_declaration_specifiers(specifiers);
        let (mut return_type, params, variadic) = resolve_function_parts(declarator, base_ty);
        if base_qualifiers.is_const {
            return_type = mark_innermost_pointer_const(return_type);
        }
        return_type = apply_base_qualifiers(return_type, base_qualifiers);

        self.items.push(SourceItem::Function(SourceFunction {
            name,
            calling_convention,
            parameters: params,
            return_type,
            variadic,
            source_offset: Some(offset),
        }));
    }

    fn extract_variable_or_type(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: &Declarator,
        is_extern: bool,
        offset: usize,
    ) {
        self.extract_inline_type_definitions(specifiers, offset);

        if is_extern {
            let name = match declarator_name(declarator) {
                Some(n) => n,
                None => return,
            };
            let ty = resolve_full_type(specifiers, declarator);
            self.items.push(SourceItem::Variable(SourceVariable {
                name,
                ty,
                source_offset: Some(offset),
            }));
        }
    }

    fn extract_inline_type_definitions(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        offset: usize,
    ) {
        for spec in specifiers {
            if let DeclarationSpecifier::TypeSpecifier(ts) = &spec.node {
                match &ts.node {
                    TypeSpecifier::Struct(st) if st.node.declarations.is_some() => {
                        self.extract_record(&st.node, offset);
                    }
                    TypeSpecifier::Enum(et) if !et.node.enumerators.is_empty() => {
                        self.extract_enum(&et.node, offset);
                    }
                    _ => {}
                }
            }
        }
    }

    fn extract_record(&mut self, st: &StructType, offset: usize) {
        let kind = match st.kind.node {
            StructKind::Struct => RecordKind::Struct,
            StructKind::Union => RecordKind::Union,
        };
        let name = st.identifier.as_ref().map(|id| id.node.name.clone());

        let fields = st.declarations.as_ref().map(|decls| {
            let mut fields = Vec::new();
            for decl in decls {
                match &decl.node {
                    StructDeclaration::Field(field) => {
                        self.extract_struct_fields(&field.node, &mut fields);
                    }
                    StructDeclaration::StaticAssert(_) => {}
                }
            }
            fields
        });

        self.items.push(SourceItem::Record(SourceRecord {
            kind,
            name,
            fields,
            source_offset: Some(offset),
        }));
    }

    fn extract_struct_fields(&mut self, field: &StructField, out: &mut Vec<SourceField>) {
        let base_type_specs: Vec<_> = field
            .specifiers
            .iter()
            .filter_map(|sq| match &sq.node {
                SpecifierQualifier::TypeSpecifier(ts) => Some(ts),
                _ => None,
            })
            .collect();

        if field.declarators.is_empty() {
            let ty = resolve_base_type_from_type_specs(&base_type_specs);
            out.push(SourceField {
                name: None,
                ty,
                bit_width: None,
            });
            return;
        }

        let base_qualifiers = qualifiers_from_specifier_qualifiers(&field.specifiers);

        for sd in &field.declarators {
            let name = sd
                .node
                .declarator
                .as_ref()
                .and_then(|d| declarator_name(&d.node));

            let bit_width = sd
                .node
                .bit_width
                .as_ref()
                .and_then(|expr| eval_const_expr(&expr.node))
                .and_then(|value| u64::try_from(value).ok());

            if sd.node.bit_width.is_some() {
                let field_name = name.as_deref().unwrap_or("<anonymous>");
                self.diagnostics.push(
                    SourceDiagnostic::warning(
                        DiagnosticKind::DeclarationPartial,
                        format!(
                            "bitfield layout partially represented on field '{}'",
                            field_name
                        ),
                    )
                    .with_item(field_name),
                );
            }

            let mut ty = match &sd.node.declarator {
                Some(d) => {
                    apply_derived_type(resolve_base_type_from_type_specs(&base_type_specs), &d.node)
                }
                None => resolve_base_type_from_type_specs(&base_type_specs),
            };
            if base_qualifiers.is_const {
                ty = mark_innermost_pointer_const(ty);
            }
            ty = apply_base_qualifiers(ty, base_qualifiers);
            out.push(SourceField {
                name,
                ty,
                bit_width,
            });
        }
    }

    fn extract_enum(&mut self, et: &EnumType, offset: usize) {
        let name = et.identifier.as_ref().map(|id| id.node.name.clone());
        let variants: Vec<SourceEnumVariant> = et
            .enumerators
            .iter()
            .map(|e| SourceEnumVariant {
                name: e.node.identifier.node.name.clone(),
                value: e
                    .node
                    .expression
                    .as_ref()
                    .and_then(|expr| eval_const_expr(&expr.node)),
            })
            .collect();

        self.items.push(SourceItem::Enum(SourceEnum {
            name,
            variants,
            source_offset: Some(offset),
        }));
    }

    // --- Diagnostic emission ---

    fn emit_specifier_diagnostics(
        &mut self,
        specifiers: &[Node<DeclarationSpecifier>],
        item_name: &str,
        offset: usize,
    ) {
        for spec in specifiers {
            match &spec.node {
                DeclarationSpecifier::TypeSpecifier(ts) => match &ts.node {
                    TypeSpecifier::Complex => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationUnsupported,
                                format!("_Complex type not supported on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                    TypeSpecifier::TypeOf(_) => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationPartial,
                                format!("typeof not resolved on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                    _ => {}
                },
                DeclarationSpecifier::Function(fs) => match &fs.node {
                    FunctionSpecifier::Inline => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationPartial,
                                format!("inline specifier ignored on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                    FunctionSpecifier::Noreturn => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationPartial,
                                format!("_Noreturn specifier ignored on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                },
                DeclarationSpecifier::Alignment(_) => {
                    self.diagnostics.push(
                        SourceDiagnostic::warning(
                            DiagnosticKind::DeclarationPartial,
                            format!("_Alignas specifier ignored on '{}'", item_name),
                        )
                        .with_item(item_name)
                        .with_location(None, offset),
                    );
                }
                DeclarationSpecifier::StorageClass(sc) => match &sc.node {
                    StorageClassSpecifier::ThreadLocal => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationPartial,
                                format!("_Thread_local ignored on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                    StorageClassSpecifier::Register => {
                        self.diagnostics.push(
                            SourceDiagnostic::warning(
                                DiagnosticKind::DeclarationPartial,
                                format!("register storage class ignored on '{}'", item_name),
                            )
                            .with_item(item_name)
                            .with_location(None, offset),
                        );
                    }
                    _ => {}
                },
                DeclarationSpecifier::TypeQualifier(_) => {}
                _ => {}
            }
        }
    }

    fn emit_derived_diagnostics(
        &mut self,
        declarator: &Declarator,
        item_name: &str,
        offset: usize,
    ) {
        for derived in &declarator.derived {
            match &derived.node {
                DerivedDeclarator::KRFunction(_) => {
                    self.diagnostics.push(
                        SourceDiagnostic::warning(
                            DiagnosticKind::DeclarationUnsupported,
                            format!("K&R function declaration on '{}'", item_name),
                        )
                        .with_item(item_name)
                        .with_location(None, offset),
                    );
                }
                DerivedDeclarator::Block(_) => {
                    self.diagnostics.push(
                        SourceDiagnostic::warning(
                            DiagnosticKind::DeclarationUnsupported,
                            format!("block pointer not supported on '{}'", item_name),
                        )
                        .with_item(item_name)
                        .with_location(None, offset),
                    );
                }
                _ => {}
            }
        }
    }

    fn emit_extension_diagnostics(
        &mut self,
        extensions: &[Node<Extension>],
        item_name: &str,
        offset: usize,
    ) {
        if extensions.is_empty() {
            return;
        }
        let attr_names: Vec<String> = extensions
            .iter()
            .filter_map(|e| match &e.node {
                Extension::Attribute(a) => {
                    if calling_convention_from_attr_name(&a.name.node).is_some() {
                        None
                    } else {
                        Some(a.name.node.clone())
                    }
                }
                Extension::AsmLabel(_) => Some("asm_label".into()),
                Extension::AvailabilityAttribute(_) => Some("availability".into()),
            })
            .collect();
        if !attr_names.is_empty() {
            self.diagnostics.push(
                SourceDiagnostic::warning(
                    DiagnosticKind::DeclarationPartial,
                    format!("attributes ignored: {}", attr_names.join(", ")),
                )
                .with_item(item_name)
                .with_location(None, offset),
            );
        }
    }
}

// --- Free functions ---

fn is_function_declarator(declarator: &Declarator) -> bool {
    declarator
        .derived
        .iter()
        .any(|d| matches!(d.node, DerivedDeclarator::Function(_)))
}

fn declarator_name(declarator: &Declarator) -> Option<String> {
    match &declarator.kind.node {
        DeclaratorKind::Identifier(id) => Some(id.node.name.clone()),
        DeclaratorKind::Declarator(inner) => declarator_name(&inner.node),
        DeclaratorKind::Abstract => None,
    }
}

fn resolve_function_parts(
    declarator: &Declarator,
    base_return_type: SourceType,
) -> (SourceType, Vec<SourceParameter>, bool) {
    let mut return_type = base_return_type;
    let mut params = Vec::new();
    let mut variadic = false;

    for derived in &declarator.derived {
        match &derived.node {
            DerivedDeclarator::Pointer(_) => {
                return_type = SourceType::Pointer {
                    pointee: Box::new(return_type),
                    qualifiers: TypeQualifiers::default(),
                };
            }
            DerivedDeclarator::Function(fdecl) => {
                params = extract_parameters(&fdecl.node.parameters);
                variadic = fdecl.node.ellipsis == Ellipsis::Some;
            }
            _ => {}
        }
    }

    (return_type, params, variadic)
}

fn extract_parameters(params: &[Node<ParameterDeclaration>]) -> Vec<SourceParameter> {
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
            let name = p
                .node
                .declarator
                .as_ref()
                .and_then(|d| declarator_name(&d.node));
            let base = resolve_base_type_from_param_specifiers(&p.node.specifiers);
            let base_qualifiers = qualifiers_from_declaration_specifiers(&p.node.specifiers);
            let mut ty = match &p.node.declarator {
                Some(d) => apply_derived_type(base, &d.node),
                None => base,
            };
            if base_qualifiers.is_const {
                ty = mark_innermost_pointer_const(ty);
            }
            ty = apply_base_qualifiers(ty, base_qualifiers);
            SourceParameter { name, ty }
        })
        .collect()
}

fn detect_calling_convention(extensions: &[Node<Extension>]) -> Option<CallingConvention> {
    extensions
        .iter()
        .find_map(|extension| match &extension.node {
            Extension::Attribute(attribute) => {
                calling_convention_from_attr_name(&attribute.name.node)
            }
            _ => None,
        })
}

fn calling_convention_from_attr_name(name: &str) -> Option<CallingConvention> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "cdecl" | "__cdecl" => Some(CallingConvention::Cdecl),
        "stdcall" | "__stdcall" => Some(CallingConvention::Stdcall),
        "fastcall" | "__fastcall" => Some(CallingConvention::Fastcall),
        "vectorcall" | "__vectorcall" => Some(CallingConvention::Vectorcall),
        "thiscall" | "__thiscall" => Some(CallingConvention::Thiscall),
        _ => None,
    }
}

// --- Public API functions ---

fn format_parse_error(
    source: &str,
    line: usize,
    column: usize,
    expected: &impl std::fmt::Debug,
) -> String {
    let mut message = format!("parse error at line {}:{}: {:?}", line, column, expected);
    if source.contains("(^") {
        message.push_str("; block pointer declarators are currently unsupported");
    }
    message
}

/// Parse C source and extract a `SourcePackage`.
pub fn extract_from_source(source: &str) -> Result<SourcePackage, String> {
    let unit = crate::parse::translation_unit(source, crate::driver::Flavor::GnuC11)
        .map_err(|e| format_parse_error(source, e.line, e.column, &e.expected))?;

    let extractor = Extractor::new();
    let (items, diagnostics) = extractor.extract(&unit);

    Ok(SourcePackage {
        items,
        diagnostics,
        ..SourcePackage::new()
    })
}

/// Extract a `SourcePackage` from an already-parsed translation unit.
pub fn extract_from_translation_unit(
    unit: &TranslationUnit,
    source_path: Option<String>,
) -> SourcePackage {
    let extractor = Extractor::new();
    let (items, diagnostics) = extractor.extract(unit);

    SourcePackage {
        source_path,
        items,
        diagnostics,
        ..SourcePackage::new()
    }
}

/// Parse and extract in one step, with configurable flavor.
pub fn parse_and_extract(
    source: &str,
    flavor: crate::driver::Flavor,
) -> Result<SourcePackage, String> {
    let unit = crate::parse::translation_unit(source, flavor)
        .map_err(|e| format_parse_error(source, e.line, e.column, &e.expected))?;

    let extractor = Extractor::new();
    let (items, diagnostics) = extractor.extract(&unit);

    Ok(SourcePackage {
        items,
        diagnostics,
        ..SourcePackage::new()
    })
}

/// Parse with resilient recovery and extract, returning diagnostics for parse errors.
pub fn parse_and_extract_resilient(source: &str, flavor: crate::driver::Flavor) -> SourcePackage {
    let unit = crate::parse::translation_unit_resilient(source, flavor);
    let extractor = Extractor::new();
    let (items, diagnostics) = extractor.extract(&unit);

    SourcePackage {
        items,
        diagnostics,
        ..SourcePackage::new()
    }
}

/// Read a file and extract a `SourcePackage` from it.
pub fn extract_file(
    path: impl AsRef<std::path::Path>,
    flavor: crate::driver::Flavor,
) -> Result<SourcePackage, String> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    let mut pkg = parse_and_extract(&source, flavor)?;
    pkg.source_path = Some(path.display().to_string());
    Ok(pkg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn extract(source: &str) -> SourcePackage {
        extract_from_source(source).expect("parse failed")
    }

    #[test]
    fn extract_typedef_int() {
        let pkg = extract("typedef int my_int;");
        assert_eq!(pkg.items.len(), 1);
        match &pkg.items[0] {
            SourceItem::TypeAlias(ta) => {
                assert_eq!(ta.name, "my_int");
                assert_eq!(ta.target, SourceType::Int);
            }
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn extract_typedef_pointer() {
        let pkg = extract("typedef void *handle_t;");
        match &pkg.items[0] {
            SourceItem::TypeAlias(ta) => {
                assert_eq!(ta.name, "handle_t");
                assert_eq!(ta.target, SourceType::ptr(SourceType::Void));
            }
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn extract_typedef_unsigned_long() {
        let pkg = extract("typedef unsigned long size_t;");
        match &pkg.items[0] {
            SourceItem::TypeAlias(ta) => {
                assert_eq!(ta.name, "size_t");
                assert_eq!(ta.target, SourceType::ULong);
            }
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn extract_extern_function() {
        let pkg = extract("extern int puts(const char *s);");
        assert_eq!(pkg.items.len(), 1);
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.name, "puts");
                assert_eq!(f.return_type, SourceType::Int);
                assert!(!f.variadic);
                assert_eq!(f.parameters.len(), 1);
                assert_eq!(f.parameters[0].name.as_deref(), Some("s"));
                assert_eq!(f.parameters[0].ty, SourceType::const_ptr(SourceType::Char));
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_variadic_function() {
        let pkg = extract("int printf(const char *fmt, ...);");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.name, "printf");
                assert!(f.variadic);
                assert_eq!(f.parameters.len(), 1);
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_void_function() {
        let pkg = extract("void abort(void);");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.name, "abort");
                assert_eq!(f.return_type, SourceType::Void);
                assert!(f.parameters.is_empty());
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_enum_declaration() {
        let pkg = extract("enum color { RED, GREEN = 5, BLUE };");
        let enums: Vec<_> = pkg.enums().collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name.as_deref(), Some("color"));
        assert_eq!(enums[0].variants.len(), 3);
        assert_eq!(enums[0].variants[0].name, "RED");
        assert_eq!(enums[0].variants[0].value, None);
        assert_eq!(enums[0].variants[1].name, "GREEN");
        assert_eq!(enums[0].variants[1].value, Some(5));
    }

    #[test]
    fn extract_struct_with_fields() {
        let pkg = extract("struct point { int x; int y; };");
        let records: Vec<_> = pkg.records().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, RecordKind::Struct);
        assert_eq!(records[0].name.as_deref(), Some("point"));
        assert!(!records[0].is_opaque());
        let fields = records[0].fields.as_ref().unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name.as_deref(), Some("x"));
        assert_eq!(fields[0].ty, SourceType::Int);
    }

    #[test]
    fn extract_opaque_struct() {
        let pkg = extract("struct FILE;");
        let records: Vec<_> = pkg.records().collect();
        assert_eq!(records.len(), 1);
        assert!(records[0].is_opaque());
        assert_eq!(records[0].name.as_deref(), Some("FILE"));
    }

    #[test]
    fn extract_union() {
        let pkg = extract("union data { int i; float f; };");
        let records: Vec<_> = pkg.records().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, RecordKind::Union);
    }

    #[test]
    fn extract_bitfield() {
        let pkg = extract("struct flags { unsigned value:3; unsigned other:5; };");
        let record = pkg.records().next().unwrap();
        let fields = record.fields.as_ref().unwrap();
        assert_eq!(fields[0].bit_width, Some(3));
        assert_eq!(fields[1].bit_width, Some(5));
        assert!(pkg
            .diagnostics
            .iter()
            .any(|d| d.message.contains("bitfield")));
    }

    #[test]
    fn extract_function_returning_pointer() {
        let pkg = extract("void *malloc(unsigned long size);");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.return_type, SourceType::ptr(SourceType::Void));
                assert_eq!(f.parameters[0].ty, SourceType::ULong);
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_extern_variable() {
        let pkg = extract("extern int errno;");
        match &pkg.items[0] {
            SourceItem::Variable(v) => {
                assert_eq!(v.name, "errno");
                assert_eq!(v.ty, SourceType::Int);
            }
            other => panic!("expected Variable, got {:?}", other),
        }
    }

    #[test]
    fn extract_typedef_struct() {
        let pkg = extract("typedef struct point { int x; int y; } point_t;");
        let records: Vec<_> = pkg.records().collect();
        let aliases: Vec<_> = pkg.type_aliases().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(aliases.len(), 1);
        assert_eq!(records[0].name.as_deref(), Some("point"));
        assert_eq!(aliases[0].name, "point_t");
        assert_eq!(aliases[0].target, SourceType::RecordRef("point".into()));
    }

    #[test]
    fn extract_multiple_declarations() {
        let src = r#"
            typedef unsigned long size_t;
            void *malloc(size_t size);
            void free(void *ptr);
            struct FILE;
            extern int errno;
        "#;
        let pkg = extract(src);
        assert_eq!(pkg.items.len(), 5);
    }

    #[test]
    fn extract_function_pointer_typedef() {
        let pkg = extract("typedef void (*handler_t)(int sig);");
        match &pkg.items[0] {
            SourceItem::TypeAlias(ta) => {
                assert_eq!(ta.name, "handler_t");
                match &ta.target {
                    SourceType::Pointer { pointee, .. } => match pointee.as_ref() {
                        SourceType::FunctionPointer {
                            return_type,
                            parameters,
                            variadic,
                        } => {
                            assert_eq!(**return_type, SourceType::Void);
                            assert_eq!(parameters.len(), 1);
                            assert_eq!(parameters[0], SourceType::Int);
                            assert!(!variadic);
                        }
                        other => panic!("expected FunctionPointer, got {:?}", other),
                    },
                    other => panic!("expected Pointer, got {:?}", other),
                }
            }
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn extract_const_char_pointer_param() {
        let pkg = extract("int puts(const char *s);");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.parameters[0].ty, SourceType::const_ptr(SourceType::Char));
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_const_void_pointer_return() {
        let pkg = extract("const void *memchr(const void *s, int c, unsigned long n);");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.return_type, SourceType::const_ptr(SourceType::Void));
                assert_eq!(f.parameters[0].ty, SourceType::const_ptr(SourceType::Void));
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn extract_const_field_pointer() {
        let pkg = extract("struct s { const char *name; int *data; };");
        let record = pkg.records().next().unwrap();
        let fields = record.fields.as_ref().unwrap();
        assert_eq!(fields[0].ty, SourceType::const_ptr(SourceType::Char));
        assert_eq!(fields[1].ty, SourceType::ptr(SourceType::Int));
    }

    #[test]
    fn diag_static_function() {
        let pkg = extract("static int helper(void) { return 0; }");
        let funcs: Vec<_> = pkg.functions().collect();
        assert!(funcs.is_empty());
        assert_eq!(pkg.diagnostics.len(), 1);
        assert_eq!(
            pkg.diagnostics[0].kind,
            DiagnosticKind::DeclarationUnsupported
        );
    }

    #[test]
    fn captures_atomic_qualifier() {
        let pkg = extract("_Atomic int counter(void);");
        match &pkg.items[0] {
            SourceItem::Function(f) => assert_eq!(
                f.return_type,
                SourceType::Qualified {
                    ty: Box::new(SourceType::Int),
                    qualifiers: TypeQualifiers {
                        is_atomic: true,
                        ..Default::default()
                    },
                }
            ),
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn captures_volatile_qualifier() {
        let pkg = extract("volatile int flag(void);");
        match &pkg.items[0] {
            SourceItem::Function(f) => assert_eq!(
                f.return_type,
                SourceType::Qualified {
                    ty: Box::new(SourceType::Int),
                    qualifiers: TypeQualifiers {
                        is_volatile: true,
                        ..Default::default()
                    },
                }
            ),
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn captures_calling_convention() {
        let pkg = extract("int api(void) __attribute__((stdcall));");
        match &pkg.items[0] {
            SourceItem::Function(f) => {
                assert_eq!(f.calling_convention, CallingConvention::Stdcall);
            }
            other => panic!("expected Function, got {:?}", other),
        }
    }

    #[test]
    fn diag_inline_specifier() {
        let pkg = extract("inline int fast(void) { return 0; }");
        assert!(pkg.diagnostics.iter().any(|d| d.message.contains("inline")));
    }

    #[test]
    fn diag_noreturn_specifier() {
        let pkg = extract("_Noreturn void die(void);");
        assert!(pkg
            .diagnostics
            .iter()
            .any(|d| d.message.contains("_Noreturn")));
    }

    #[test]
    fn diag_static_assert() {
        let pkg = extract("_Static_assert(1, \"ok\");");
        assert!(pkg
            .diagnostics
            .iter()
            .any(|d| d.message.contains("_Static_assert")));
    }

    #[test]
    fn eval_enum_bitwise_ops() {
        let pkg =
            extract("enum flags { A = 1 << 0, B = 1 << 1, C = 1 << 2, AB = (1 << 0) | (1 << 1) };");
        let e = pkg.enums().next().unwrap();
        assert_eq!(e.variants[0].value, Some(1));
        assert_eq!(e.variants[1].value, Some(2));
        assert_eq!(e.variants[2].value, Some(4));
        assert_eq!(e.variants[3].value, Some(3));
    }

    #[test]
    fn eval_enum_comparison_ops() {
        let pkg = extract("enum cmp { LT = (1 < 2), EQ = (2 == 2), NE = (1 != 1) };");
        let e = pkg.enums().next().unwrap();
        assert_eq!(e.variants[0].value, Some(1));
        assert_eq!(e.variants[1].value, Some(1));
        assert_eq!(e.variants[2].value, Some(0));
    }

    #[test]
    fn eval_enum_logical_ops() {
        let pkg = extract("enum logic { AND = (1 && 0), OR = (0 || 1), NOT = (!0) };");
        let e = pkg.enums().next().unwrap();
        assert_eq!(e.variants[0].value, Some(0));
        assert_eq!(e.variants[1].value, Some(1));
        assert_eq!(e.variants[2].value, Some(1));
    }

    #[test]
    fn eval_enum_ternary() {
        let pkg = extract("enum tern { X = (1 > 0) ? 42 : 99 };");
        let e = pkg.enums().next().unwrap();
        assert_eq!(e.variants[0].value, Some(42));
    }

    #[test]
    fn eval_enum_modulo() {
        let pkg = extract("enum m { A = 10 % 3 };");
        let e = pkg.enums().next().unwrap();
        assert_eq!(e.variants[0].value, Some(1));
    }

    #[test]
    fn ir_determinism() {
        let src = r#"
            typedef int int32_t;
            enum status { OK = 0, ERR = 1 };
            struct point { int x; int y; };
            void *malloc(unsigned long size);
        "#;
        let pkg1 = extract(src);
        let pkg2 = extract(src);
        let json1 = serde_json::to_string(&pkg1).unwrap();
        let json2 = serde_json::to_string(&pkg2).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn extract_from_translation_unit_works() {
        let unit = crate::parse::translation_unit("int foo(void);", crate::driver::Flavor::StdC11)
            .unwrap();
        let pkg = super::extract_from_translation_unit(&unit, Some("test.h".into()));
        assert_eq!(pkg.source_path.as_deref(), Some("test.h"));
        assert_eq!(pkg.functions().count(), 1);
    }

    #[test]
    fn parse_and_extract_works() {
        let pkg = super::parse_and_extract(
            "typedef int my_int;\nvoid foo(my_int x);",
            crate::driver::Flavor::StdC11,
        )
        .unwrap();
        assert_eq!(pkg.type_aliases().count(), 1);
        assert_eq!(pkg.functions().count(), 1);
    }

    #[test]
    fn parse_and_extract_error() {
        let result = super::parse_and_extract("@@@invalid", crate::driver::Flavor::StdC11);
        assert!(result.is_err());
    }

    #[test]
    fn parse_and_extract_empty() {
        let pkg = super::parse_and_extract("", crate::driver::Flavor::StdC11).unwrap();
        assert!(pkg.is_empty());
    }

    #[test]
    fn parse_and_extract_resilient_recovers() {
        let pkg = super::parse_and_extract_resilient(
            "int before;\n@@@invalid@@@;\nint after(void);",
            crate::driver::Flavor::StdC11,
        );
        // Should recover at least some items
        assert!(pkg.item_count() >= 1);
    }

    #[test]
    fn parse_and_extract_deterministic() {
        let src = "struct point { int x; int y; };\nvoid foo(struct point *p);";
        let pkg1 = super::parse_and_extract(src, crate::driver::Flavor::GnuC11).unwrap();
        let pkg2 = super::parse_and_extract(src, crate::driver::Flavor::GnuC11).unwrap();
        let j1 = serde_json::to_string(&pkg1).unwrap();
        let j2 = serde_json::to_string(&pkg2).unwrap();
        assert_eq!(j1, j2);
    }

    #[test]
    fn extract_file_works() {
        let dir = std::env::temp_dir().join("pac_test_extract_file");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.h");
        std::fs::write(&path, "int foo(void);\nstruct bar { int x; };\n").unwrap();

        let pkg = super::extract_file(&path, crate::driver::Flavor::GnuC11).unwrap();
        assert_eq!(pkg.function_count(), 1);
        assert_eq!(pkg.record_count(), 1);
        assert_eq!(
            pkg.source_path.as_deref(),
            Some(path.display().to_string().as_str())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_file_missing_returns_error() {
        let result = super::extract_file("/nonexistent/file.h", crate::driver::Flavor::GnuC11);
        assert!(result.is_err());
    }
}
