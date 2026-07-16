//! Deterministic lowering from the parser AST into the checked source contract.
//!
//! This module is intentionally crate-private. A contract package requires the
//! explicit target, effective inputs, and generated source file assembled by
//! the scan pipeline.

mod types;

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::*;
use crate::contract::*;
use crate::preprocess::{Lexer, TokenKind};
use crate::span::{Node, Span};

use types::{
    code, declarator_name, declarator_name_span, eval_const_expr, eval_exact_integer,
    is_function_declarator, TypeResolver,
};

pub(crate) struct ExtractionContext<'a> {
    pub source: &'a str,
    pub generated_file: FileId,
    pub target: TargetFingerprint,
    pub int128_supported: bool,
    pub default_visibility: Visibility,
}

pub(crate) struct ExtractionOutput {
    pub declarations: Vec<SourceDeclaration>,
    pub diagnostics: Vec<SourceDiagnostic>,
}

pub(crate) fn extract_contract(
    unit: &TranslationUnit,
    context: ExtractionContext<'_>,
) -> ExtractionOutput {
    let mut extractor = ContractExtractor::new(context);
    extractor.index_translation_unit(unit);
    extractor.lower_translation_unit(unit);
    extractor.finish()
}

#[derive(Clone)]
struct DeclarationMeta {
    identity: DeclarationIdentity,
    name: Option<SourceName>,
    linkage: Linkage,
}

struct DeclarationDraft {
    meta: DeclarationMeta,
    visibility: Visibility,
    occurrences: Vec<DeclarationOccurrence>,
    support: SupportStatus,
    kind: Option<SourceDeclarationKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RedeclarationAction {
    Keep,
    Replace,
    Conflict,
}

struct ContractExtractor<'a> {
    context: ExtractionContext<'a>,
    ordinary: BTreeMap<String, DeclarationId>,
    tags: BTreeMap<String, DeclarationId>,
    anonymous_tags: BTreeMap<(usize, usize), DeclarationId>,
    pointer_aliases: BTreeSet<DeclarationId>,
    metadata: BTreeMap<DeclarationId, DeclarationMeta>,
    drafts: BTreeMap<DeclarationId, DeclarationDraft>,
    anonymous_ordinals: BTreeMap<Vec<String>, u64>,
    occurrence_ordinals: BTreeMap<(DeclarationId, Vec<String>), u64>,
    diagnostics: Vec<SourceDiagnostic>,
}

impl<'a> ContractExtractor<'a> {
    fn new(context: ExtractionContext<'a>) -> Self {
        Self {
            context,
            ordinary: BTreeMap::new(),
            tags: BTreeMap::new(),
            anonymous_tags: BTreeMap::new(),
            pointer_aliases: BTreeSet::new(),
            metadata: BTreeMap::new(),
            drafts: BTreeMap::new(),
            anonymous_ordinals: BTreeMap::new(),
            occurrence_ordinals: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn index_translation_unit(&mut self, unit: &TranslationUnit) {
        for external in &unit.0 {
            match &external.node {
                ExternalDeclaration::Declaration(declaration) => {
                    self.index_declaration(&declaration.node)
                }
                ExternalDeclaration::FunctionDefinition(function) => {
                    self.index_type_specifiers(&function.node.specifiers);
                    if let Some(name) = declarator_name(&function.node.declarator.node) {
                        let is_static =
                            has_storage(&function.node.specifiers, StorageClassSpecifier::Static);
                        self.index_ordinary(&name, is_static, Linkage::External);
                    }
                }
                ExternalDeclaration::StaticAssert(_) => {}
            }
        }
    }

    fn index_declaration(&mut self, declaration: &Declaration) {
        self.index_type_specifiers(&declaration.specifiers);
        let is_typedef = has_storage(&declaration.specifiers, StorageClassSpecifier::Typedef);
        let is_static = has_storage(&declaration.specifiers, StorageClassSpecifier::Static);
        for declarator in &declaration.declarators {
            if let Some(name) = declarator_name(&declarator.node.declarator.node) {
                self.index_ordinary(
                    &name,
                    is_static,
                    if is_typedef {
                        Linkage::None
                    } else {
                        Linkage::External
                    },
                );
            }
        }
    }

    fn index_type_specifiers(&mut self, specifiers: &[Node<DeclarationSpecifier>]) {
        for specifier in specifiers {
            if let DeclarationSpecifier::TypeSpecifier(type_specifier) = &specifier.node {
                self.index_type_specifier(type_specifier);
            }
        }
    }

    fn index_field_type_specifiers(&mut self, specifiers: &[Node<SpecifierQualifier>]) {
        for specifier in specifiers {
            if let SpecifierQualifier::TypeSpecifier(type_specifier) = &specifier.node {
                self.index_type_specifier(type_specifier);
            }
        }
    }

    fn index_type_specifier(&mut self, type_specifier: &Node<TypeSpecifier>) {
        match &type_specifier.node {
            TypeSpecifier::Struct(record) => {
                let id = self.index_tag(
                    record
                        .node
                        .identifier
                        .as_ref()
                        .map(|identifier| identifier.node.name.as_str()),
                    record.span,
                );
                let _ = id;
                if let Some(declarations) = &record.node.declarations {
                    for declaration in declarations {
                        if let StructDeclaration::Field(field) = &declaration.node {
                            self.index_field_type_specifiers(&field.node.specifiers);
                        }
                    }
                }
            }
            TypeSpecifier::Enum(enumeration) => {
                self.index_tag(
                    enumeration
                        .node
                        .identifier
                        .as_ref()
                        .map(|identifier| identifier.node.name.as_str()),
                    enumeration.span,
                );
            }
            TypeSpecifier::Atomic(type_name) => {
                self.index_field_type_specifiers(&type_name.node.specifiers)
            }
            _ => {}
        }
    }

    fn index_ordinary(&mut self, name: &str, internal: bool, linkage: Linkage) -> DeclarationId {
        if let Some(id) = self.ordinary.get(name) {
            return *id;
        }
        let scope = if internal {
            EntityScope::File(self.context.generated_file)
        } else {
            EntityScope::TranslationUnit
        };
        let entity = EntityId::named(EntityNamespace::Ordinary, scope, name)
            .expect("parser identifiers are nonempty");
        let id = DeclarationId::from_entity(entity);
        let normalized = normalize_identifier(name).expect("parser identifiers are nonempty");
        let meta = DeclarationMeta {
            identity: DeclarationIdentity::Named {
                namespace: EntityNamespace::Ordinary,
                scope,
                normalized_name: normalized.clone(),
            },
            name: Some(SourceName {
                normalized,
                original: name.to_owned(),
            }),
            linkage: if internal { Linkage::Internal } else { linkage },
        };
        self.ordinary.insert(name.to_owned(), id);
        self.metadata.insert(id, meta);
        id
    }

    fn index_tag(&mut self, name: Option<&str>, span: Span) -> DeclarationId {
        if let Some(name) = name {
            if let Some(id) = self.tags.get(name) {
                return *id;
            }
            let scope = EntityScope::TranslationUnit;
            let entity = EntityId::named(EntityNamespace::Tag, scope, name)
                .expect("parser identifiers are nonempty");
            let id = DeclarationId::from_entity(entity);
            let normalized = normalize_identifier(name).expect("parser identifiers are nonempty");
            self.tags.insert(name.to_owned(), id);
            self.metadata.insert(
                id,
                DeclarationMeta {
                    identity: DeclarationIdentity::Named {
                        namespace: EntityNamespace::Tag,
                        scope,
                        normalized_name: normalized.clone(),
                    },
                    name: Some(SourceName {
                        normalized,
                        original: name.to_owned(),
                    }),
                    linkage: Linkage::None,
                },
            );
            return id;
        }

        if let Some(id) = self.anonymous_tags.get(&(span.start, span.end)) {
            return *id;
        }
        let scope = EntityScope::TranslationUnit;
        let tokens = match self.tokens(span) {
            Some(tokens) => tokens,
            None => {
                let diagnostic = self.diagnostic(
                    "PARC-E1203",
                    DiagnosticStage::Extract,
                    Severity::Error,
                    DiagnosticCompletenessImpact::ForcesRejected,
                    "anonymous tag did not contain a usable generated-source span",
                    None,
                    None,
                );
                self.diagnostics.push(diagnostic);
                Vec::new()
            }
        };
        let ordinal_entry = self.anonymous_ordinals.entry(tokens.clone()).or_insert(0);
        let duplicate_ordinal = *ordinal_entry;
        *ordinal_entry += 1;
        let token_bytes = canonical_tokens_bytes(&tokens);
        let token_fingerprint = ContentFingerprint::from_content(&token_bytes);
        let id = DeclarationId::from_entity(EntityId::anonymous(
            scope,
            token_fingerprint.as_bytes(),
            duplicate_ordinal,
        ));
        self.anonymous_tags.insert((span.start, span.end), id);
        self.metadata.insert(
            id,
            DeclarationMeta {
                identity: DeclarationIdentity::Anonymous {
                    scope,
                    token_fingerprint,
                    duplicate_ordinal,
                },
                name: None,
                linkage: Linkage::None,
            },
        );
        id
    }

    fn lower_translation_unit(&mut self, unit: &TranslationUnit) {
        for external in &unit.0 {
            match &external.node {
                ExternalDeclaration::Declaration(declaration) => {
                    self.lower_declaration(&declaration.node, external.span)
                }
                ExternalDeclaration::FunctionDefinition(function) => {
                    self.lower_function_definition(&function.node, external.span)
                }
                ExternalDeclaration::StaticAssert(_) => self.diagnostics.push(self.diagnostic(
                    "PARC-P1200",
                    DiagnosticStage::Extract,
                    Severity::Note,
                    DiagnosticCompletenessImpact::Informational,
                    "_Static_assert has no bindable declaration",
                    self.range(external.span),
                    None,
                )),
            }
        }
    }

    fn lower_declaration(&mut self, declaration: &Declaration, _span: Span) {
        self.lower_type_specifiers(&declaration.specifiers);
        if declaration.declarators.is_empty() {
            return;
        }
        let storage = declaration_storage(&declaration.specifiers);
        let is_typedef = storage == StorageClass::Typedef;
        for init in &declaration.declarators {
            let declarator = &init.node.declarator.node;
            let Some(name) = declarator_name(declarator) else {
                self.push_unbound_declarator_diagnostic(
                    init.span,
                    "a top-level declarator has no bindable identifier",
                );
                continue;
            };
            let Some(id) = self.ordinary.get(&name).copied() else {
                self.push_unbound_declarator_diagnostic(
                    init.span,
                    format!("declarator {name} was not indexed before lowering"),
                );
                continue;
            };
            let extensions = declaration_extensions(&declaration.specifiers, declarator);
            self.add_occurrence(
                id,
                init.span,
                declarator_name_span(declarator),
                storage,
                declaration_is_definition(
                    is_typedef,
                    is_function_declarator(declarator),
                    storage,
                    init.node.initializer.is_some(),
                ),
                &extensions,
            );
            let resolver = TypeResolver::new(
                &self.ordinary,
                &self.tags,
                &self.anonymous_tags,
                &self.pointer_aliases,
                self.context.source,
                self.context.int128_supported,
            );
            let kind = if is_typedef {
                SourceDeclarationKind::TypeAlias(SourceTypeAlias {
                    target: resolver.declaration_type(&declaration.specifiers, Some(declarator)),
                })
            } else if is_function_declarator(declarator) {
                self.lower_function_kind(
                    &resolver,
                    &declaration.specifiers,
                    declarator,
                    &name,
                    &extensions,
                )
            } else {
                let ty = resolver.declaration_type(&declaration.specifiers, Some(declarator));
                match asm_link_name(&extensions) {
                    Ok(link_name) => SourceDeclarationKind::Variable(SourceVariable {
                        link_name: link_name.unwrap_or_else(|| name.clone()),
                        ty,
                        thread_local: has_storage(
                            &declaration.specifiers,
                            StorageClassSpecifier::ThreadLocal,
                        ),
                    }),
                    Err(spelling) => SourceDeclarationKind::Unsupported(SourceUnsupported {
                        category: UnsupportedDeclarationCategory::UnsupportedExtension,
                        spelling,
                        diagnostic: code("PARC-E1211"),
                    }),
                }
            };
            self.set_kind(id, kind);
            self.apply_extension_support(id, &extensions);
            self.apply_specifier_support(id, &declaration.specifiers);
        }
    }

    fn lower_function_definition(&mut self, function: &FunctionDefinition, span: Span) {
        self.lower_type_specifiers(&function.specifiers);
        let Some(name) = declarator_name(&function.declarator.node) else {
            self.push_unbound_declarator_diagnostic(
                span,
                "a function definition has no bindable identifier",
            );
            return;
        };
        let Some(id) = self.ordinary.get(&name).copied() else {
            self.push_unbound_declarator_diagnostic(
                span,
                format!("function {name} was not indexed before lowering"),
            );
            return;
        };
        let extensions = declaration_extensions(&function.specifiers, &function.declarator.node);
        self.add_occurrence(
            id,
            span,
            declarator_name_span(&function.declarator.node),
            declaration_storage(&function.specifiers),
            true,
            &extensions,
        );
        let resolver = TypeResolver::new(
            &self.ordinary,
            &self.tags,
            &self.anonymous_tags,
            &self.pointer_aliases,
            self.context.source,
            self.context.int128_supported,
        );
        let kind = self.lower_function_kind(
            &resolver,
            &function.specifiers,
            &function.declarator.node,
            &name,
            &extensions,
        );
        self.set_kind(id, kind);
        self.apply_extension_support(id, &extensions);
        self.apply_specifier_support(id, &function.specifiers);
    }

    fn lower_function_kind(
        &self,
        resolver: &TypeResolver<'_>,
        specifiers: &[Node<DeclarationSpecifier>],
        declarator: &Declarator,
        name: &str,
        extensions: &[Node<Extension>],
    ) -> SourceDeclarationKind {
        let convention = match calling_convention(extensions) {
            Ok(convention) => convention,
            Err(spelling) => {
                return SourceDeclarationKind::Unsupported(SourceUnsupported {
                    category: UnsupportedDeclarationCategory::UnsupportedExtension,
                    spelling,
                    diagnostic: code("PARC-E1214"),
                })
            }
        };
        let Some(parts) = resolver.function_parts(specifiers, declarator) else {
            return SourceDeclarationKind::Unsupported(SourceUnsupported {
                category: UnsupportedDeclarationCategory::InvalidDeclaration,
                spelling: resolver.text(declarator.kind.span, "function declarator"),
                diagnostic: code("PARC-E1201"),
            });
        };
        let parent = self.ordinary[name];
        let parameters = parts
            .parameters
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                let range = self.range(parameter.span)?;
                let ordinal = u64::try_from(index).ok()?;
                let source_name = parameter.name.as_ref().map(|name| SourceName {
                    normalized: normalize_identifier(name).expect("parser identifier"),
                    original: name.clone(),
                });
                let id = ChildId::parameter(parent, ordinal);
                let lowered_attributes = attributes_from_extensions(
                    self.context.source,
                    self.context.generated_file,
                    &parameter.extensions,
                );
                let mut support =
                    nested_type_support(&parameter.ty).unwrap_or(SupportStatus::Supported);
                if lowered_attributes.invalid_span {
                    support = unsupported_status(
                        "PARC-E1210",
                        "a parameter attribute had an invalid generated-source span",
                    );
                } else if has_unmodeled_extensions(&parameter.extensions) && support.is_supported()
                {
                    support = partial_status(
                        "PARC-P1205",
                        "parameter contains an unmodeled ABI-relevant attribute",
                    );
                }
                Some(SourceParameter {
                    id,
                    ordinal,
                    name: source_name,
                    ty: parameter.ty,
                    range,
                    provenance: generated_provenance(),
                    attributes: lowered_attributes.attributes,
                    support,
                })
            })
            .collect::<Option<Vec<_>>>();
        let Some(parameters) = parameters else {
            return SourceDeclarationKind::Unsupported(SourceUnsupported {
                category: UnsupportedDeclarationCategory::InvalidDeclaration,
                spelling: resolver.text(declarator.kind.span, "function declarator"),
                diagnostic: code("PARC-E1203"),
            });
        };
        match asm_link_name(extensions) {
            Ok(link_name) => SourceDeclarationKind::Function(SourceFunction {
                link_name: link_name.unwrap_or_else(|| name.to_owned()),
                return_type: parts.return_type,
                parameters,
                prototype: parts.prototype,
                calling_convention: convention,
            }),
            Err(spelling) => SourceDeclarationKind::Unsupported(SourceUnsupported {
                category: UnsupportedDeclarationCategory::UnsupportedExtension,
                spelling,
                diagnostic: code("PARC-E1211"),
            }),
        }
    }

    fn lower_type_specifiers(&mut self, specifiers: &[Node<DeclarationSpecifier>]) {
        for specifier in specifiers {
            if let DeclarationSpecifier::TypeSpecifier(type_specifier) = &specifier.node {
                self.lower_type_specifier(type_specifier);
            }
        }
    }

    fn lower_field_type_specifiers(&mut self, specifiers: &[Node<SpecifierQualifier>]) {
        for specifier in specifiers {
            if let SpecifierQualifier::TypeSpecifier(type_specifier) = &specifier.node {
                self.lower_type_specifier(type_specifier);
            }
        }
    }

    fn lower_type_specifier(&mut self, type_specifier: &Node<TypeSpecifier>) {
        match &type_specifier.node {
            TypeSpecifier::Struct(record) => self.lower_record(&record.node, record.span),
            TypeSpecifier::Enum(enumeration) => {
                self.lower_enum(&enumeration.node, enumeration.span)
            }
            TypeSpecifier::Atomic(type_name) => {
                self.lower_field_type_specifiers(&type_name.node.specifiers)
            }
            _ => {}
        }
    }

    fn lower_record(&mut self, record: &StructType, span: Span) {
        let id = self.tag_id(
            record
                .identifier
                .as_ref()
                .map(|identifier| identifier.node.name.as_str()),
            span,
        );
        self.add_occurrence(
            id,
            span,
            record.identifier.as_ref().map(|identifier| identifier.span),
            StorageClass::None,
            record.declarations.is_some(),
            &[],
        );

        let mut fields = Vec::new();
        if let Some(declarations) = &record.declarations {
            for declaration in declarations {
                if let StructDeclaration::Field(field) = &declaration.node {
                    self.lower_field_type_specifiers(&field.node.specifiers);
                    let resolver = TypeResolver::new(
                        &self.ordinary,
                        &self.tags,
                        &self.anonymous_tags,
                        &self.pointer_aliases,
                        self.context.source,
                        self.context.int128_supported,
                    );
                    if field.node.declarators.is_empty() {
                        if let Some(value) = self.lower_field(
                            &resolver,
                            id,
                            &field.node.specifiers,
                            None,
                            None,
                            field.span,
                            fields.len(),
                        ) {
                            fields.push(value);
                        }
                    } else {
                        for declarator in &field.node.declarators {
                            if let Some(value) = self.lower_field(
                                &resolver,
                                id,
                                &field.node.specifiers,
                                declarator.node.declarator.as_ref(),
                                declarator.node.bit_width.as_deref(),
                                declarator.span,
                                fields.len(),
                            ) {
                                fields.push(value);
                            }
                        }
                    }
                }
            }
            let field_count = fields.len();
            let mut invalid_unsized_fields = Vec::new();
            for (index, field) in fields.iter_mut().enumerate() {
                if let CTypeKind::Array { bound, .. } = &mut field.ty.kind {
                    if *bound != ArrayBound::Incomplete {
                        continue;
                    }
                    let flexible_allowed = record.kind.node == StructKind::Struct
                        && index > 0
                        && index + 1 == field_count;
                    if flexible_allowed {
                        *bound = ArrayBound::Flexible;
                    } else {
                        let spelling = generated_range_text(self.context.source, field.range)
                            .unwrap_or_else(|| field.identity_tokens.join(" "));
                        *bound = ArrayBound::Invalid {
                            spelling,
                            diagnostic: code("PARC-E1212"),
                        };
                        field.support = unsupported_status(
                            "PARC-E1212",
                            "an unsized field is only valid as the final non-first struct member",
                        );
                        invalid_unsized_fields.push(field.range);
                    }
                }
            }
            for range in invalid_unsized_fields {
                self.diagnostics.push(self.diagnostic(
                    "PARC-E1212",
                    DiagnosticStage::Extract,
                    Severity::Error,
                    DiagnosticCompletenessImpact::ForcesRejected,
                    "an unsized field is only valid as the final non-first struct member",
                    Some(range),
                    Some(id),
                ));
            }
        }
        let kind = match record.kind.node {
            StructKind::Struct => RecordKind::Struct,
            StructKind::Union => RecordKind::Union,
        };
        self.set_kind(
            id,
            SourceDeclarationKind::Record(SourceRecord {
                kind,
                completeness: if record.declarations.is_some() {
                    RecordCompleteness::Complete
                } else {
                    RecordCompleteness::Incomplete
                },
                fields,
            }),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_field(
        &self,
        resolver: &TypeResolver<'_>,
        parent: DeclarationId,
        specifiers: &[Node<SpecifierQualifier>],
        declarator: Option<&Node<Declarator>>,
        bit_width: Option<&Node<Expression>>,
        span: Span,
        index: usize,
    ) -> Option<SourceField> {
        let range = self.range(span)?;
        let name = declarator.and_then(|value| declarator_name(&value.node));
        let source_name = name.as_ref().map(|name| SourceName {
            normalized: normalize_identifier(name).expect("parser identifier"),
            original: name.clone(),
        });
        let identity_tokens = self.tokens(span)?;
        let duplicate_ordinal = u64::try_from(index).ok()?;
        let id = if let Some(name) = &source_name {
            ChildId::named(parent, ChildRole::Field, &name.normalized).ok()?
        } else {
            ChildId::anonymous(
                parent,
                ChildRole::Field,
                &canonical_tokens_bytes(&identity_tokens),
                duplicate_ordinal,
            )
        };
        let ty = resolver.field_type(specifiers, declarator.map(|value| &value.node));
        let bit_width = match bit_width {
            Some(expression) => {
                let spelling = self.text(expression.span)?;
                Some(match eval_const_expr(&expression.node) {
                    Some(value) if value >= 0 => match u64::try_from(value) {
                        Ok(bits) => BitWidth::Known { bits },
                        Err(_) => BitWidth::Invalid {
                            spelling,
                            diagnostic: code("PARC-E1207"),
                        },
                    },
                    Some(_) => BitWidth::Invalid {
                        spelling,
                        diagnostic: code("PARC-E1207"),
                    },
                    None => BitWidth::Expression {
                        normalized_expression: spelling,
                    },
                })
            }
            None => None,
        };
        let extensions = field_extensions(specifiers, declarator.map(|value| &value.node));
        let lowered_attributes = attributes_from_extensions(
            self.context.source,
            self.context.generated_file,
            &extensions,
        );
        let mut support = nested_type_support(&ty).unwrap_or(SupportStatus::Supported);
        if let Some(bit_width) = &bit_width {
            support = match bit_width {
                BitWidth::Invalid { .. } => unsupported_status(
                    "PARC-E1207",
                    "bit-field width is negative or overflows the schema-v2 range",
                ),
                BitWidth::Expression { .. } if support.is_supported() => partial_status(
                    "PARC-P1207",
                    "bit-field width could not be evaluated without guessing",
                ),
                BitWidth::Known { .. } => support,
                BitWidth::Expression { .. } => support,
            };
        }
        if lowered_attributes.invalid_span {
            support = unsupported_status(
                "PARC-E1210",
                "a field attribute had an invalid generated-source span",
            );
        } else if has_unmodeled_extensions(&extensions) && support.is_supported() {
            support = partial_status(
                "PARC-P1205",
                "field contains an unmodeled ABI-relevant attribute",
            );
        }
        Some(SourceField {
            id,
            name: source_name,
            ty,
            bit_width,
            range,
            provenance: generated_provenance(),
            attributes: lowered_attributes.attributes,
            support,
            identity_tokens,
            duplicate_ordinal,
        })
    }

    fn lower_enum(&mut self, enumeration: &EnumType, span: Span) {
        let id = self.tag_id(
            enumeration
                .identifier
                .as_ref()
                .map(|identifier| identifier.node.name.as_str()),
            span,
        );
        self.add_occurrence(
            id,
            span,
            enumeration
                .identifier
                .as_ref()
                .map(|identifier| identifier.span),
            StorageClass::None,
            !enumeration.enumerators.is_empty(),
            &[],
        );
        let mut next_value = Some(ExactInteger::signed(0));
        let mut previous_name = None::<String>;
        let variants = enumeration
            .enumerators
            .iter()
            .filter_map(|enumerator| {
                let range = self.range(enumerator.span)?;
                let original = enumerator.node.identifier.node.name.clone();
                let name = SourceName {
                    normalized: normalize_identifier(&original).ok()?,
                    original,
                };
                let variant_id =
                    ChildId::named(id, ChildRole::EnumVariant, &name.normalized).ok()?;
                let value = match &enumerator.node.expression {
                    Some(expression) => match eval_exact_integer(&expression.node) {
                        Some(value) => {
                            next_value = increment_exact_integer(value);
                            EnumValue::Evaluated { value }
                        }
                        None => {
                            next_value = None;
                            EnumValue::Unevaluated {
                                normalized_expression: self.text(expression.span)?,
                            }
                        }
                    },
                    None => match next_value {
                        Some(value) => {
                            next_value = increment_exact_integer(value);
                            EnumValue::Evaluated { value }
                        }
                        None => EnumValue::Unevaluated {
                            normalized_expression: format!(
                                "{} + 1",
                                previous_name.as_deref().unwrap_or("previous enumerator")
                            ),
                        },
                    },
                };
                previous_name = Some(name.normalized.clone());
                let lowered_attributes = attributes_from_extensions(
                    self.context.source,
                    self.context.generated_file,
                    &enumerator.node.extensions,
                );
                let mut support = if matches!(value, EnumValue::Unevaluated { .. }) {
                    partial_status(
                        "PARC-P1203",
                        "enumerator value could not be evaluated without guessing",
                    )
                } else {
                    SupportStatus::Supported
                };
                if lowered_attributes.invalid_span {
                    support = unsupported_status(
                        "PARC-E1210",
                        "an enumerator attribute had an invalid generated-source span",
                    );
                } else if has_unmodeled_extensions(&enumerator.node.extensions)
                    && support.is_supported()
                {
                    support = partial_status(
                        "PARC-P1202",
                        "enumerator has an unmodeled source attribute",
                    );
                }
                Some(SourceEnumVariant {
                    id: variant_id,
                    name,
                    value,
                    range,
                    provenance: generated_provenance(),
                    attributes: lowered_attributes.attributes,
                    support,
                    identity_tokens: self.tokens(enumerator.span)?,
                    duplicate_ordinal: 0,
                })
            })
            .collect();
        self.set_kind(
            id,
            SourceDeclarationKind::Enum(SourceEnum {
                explicit_underlying_type: None,
                variants,
            }),
        );
    }

    fn tag_id(&self, name: Option<&str>, span: Span) -> DeclarationId {
        match name {
            Some(name) => self.tags[name],
            None => self.anonymous_tags[&(span.start, span.end)],
        }
    }

    fn add_occurrence(
        &mut self,
        id: DeclarationId,
        span: Span,
        name_span: Option<Span>,
        storage: StorageClass,
        is_definition: bool,
        extensions: &[Node<Extension>],
    ) {
        let Some(range) = self.range(span) else {
            self.diagnostics.push(self.diagnostic(
                "PARC-E1203",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "parser node did not contain a usable generated-source span",
                None,
                Some(id),
            ));
            return;
        };
        let Some(normalized_tokens) = self.tokens(span) else {
            self.diagnostics.push(self.diagnostic(
                "PARC-E1203",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "parser node tokens were outside the generated source",
                Some(range),
                Some(id),
            ));
            return;
        };
        let Some(spelling) = self.text(span) else {
            self.diagnostics.push(self.diagnostic(
                "PARC-E1203",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "parser node spelling was outside the generated source",
                Some(range),
                Some(id),
            ));
            return;
        };
        let ordinal = self
            .occurrence_ordinals
            .entry((id, normalized_tokens.clone()))
            .or_insert(0);
        let duplicate_ordinal = *ordinal;
        *ordinal += 1;
        let occurrence_id = OccurrenceId::derive(
            id,
            self.context.generated_file,
            &canonical_tokens_bytes(&normalized_tokens),
            duplicate_ordinal,
        );
        let lowered_attributes = attributes_from_extensions(
            self.context.source,
            self.context.generated_file,
            extensions,
        );
        if lowered_attributes.invalid_span {
            let diagnostic = self.diagnostic(
                "PARC-E1210",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "a declaration attribute had an invalid generated-source span",
                Some(range),
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
        let occurrence = DeclarationOccurrence {
            id: occurrence_id,
            range,
            name_range: name_span.and_then(|span| self.range(span)),
            spelling,
            normalized_tokens,
            duplicate_ordinal,
            storage,
            is_definition,
            attributes: lowered_attributes.attributes,
            provenance: generated_provenance(),
        };
        let visibility = visibility_from_extensions(self.context.source, extensions)
            .ok()
            .flatten();
        let draft = self.draft(id);
        draft.occurrences.push(occurrence);
        if let Some(visibility) = visibility {
            draft.visibility = visibility;
        }
    }

    fn set_kind(&mut self, id: DeclarationId, kind: SourceDeclarationKind) {
        let support = declaration_kind_support(&kind).unwrap_or(SupportStatus::Supported);
        let mut conflict_range = None;
        {
            let draft = self.draft(id);
            if !support.is_supported() {
                draft.support = support;
            }
            let action = draft
                .kind
                .as_ref()
                .map_or(RedeclarationAction::Replace, |existing| {
                    redeclaration_action(existing, &kind)
                });
            match action {
                RedeclarationAction::Keep => {}
                RedeclarationAction::Replace => draft.kind = Some(kind),
                RedeclarationAction::Conflict => {
                    conflict_range = draft.occurrences.last().map(|occurrence| occurrence.range);
                    let spelling = draft
                        .occurrences
                        .last()
                        .map(|occurrence| occurrence.spelling.clone())
                        .unwrap_or_else(|| "conflicting redeclaration".to_owned());
                    draft.kind = Some(SourceDeclarationKind::Unsupported(SourceUnsupported {
                        category: UnsupportedDeclarationCategory::InvalidDeclaration,
                        spelling,
                        diagnostic: code("PARC-E1204"),
                    }));
                    draft.support = unsupported_status(
                        "PARC-E1204",
                        "one entity has semantically incompatible redeclarations",
                    );
                }
            }
        }
        if conflict_range.is_some() {
            let diagnostic = self.diagnostic(
                "PARC-E1204",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "one entity has semantically incompatible redeclarations",
                conflict_range,
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
        let pointer_alias = self
            .drafts
            .get(&id)
            .and_then(|draft| draft.kind.as_ref())
            .is_some_and(|kind| match kind {
                SourceDeclarationKind::TypeAlias(alias) => {
                    type_is_effectively_pointer(&alias.target, &self.pointer_aliases)
                }
                _ => false,
            });
        if pointer_alias {
            self.pointer_aliases.insert(id);
        } else {
            self.pointer_aliases.remove(&id);
        }
    }

    fn apply_extension_support(&mut self, id: DeclarationId, extensions: &[Node<Extension>]) {
        let range = self
            .drafts
            .get(&id)
            .and_then(|draft| draft.occurrences.last())
            .map(|occurrence| occurrence.range);
        if asm_link_name(extensions).is_err() {
            self.draft(id).support = unsupported_status(
                "PARC-E1211",
                "asm link label could not be decoded without target-dependent guessing",
            );
            let diagnostic = self.diagnostic(
                "PARC-E1211",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "asm link label could not be decoded without target-dependent guessing",
                range,
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
        if calling_convention(extensions).is_err() {
            self.draft(id).support = unsupported_status(
                "PARC-E1214",
                "calling-convention attributes are malformed or conflict",
            );
            let diagnostic = self.diagnostic(
                "PARC-E1214",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "calling-convention attributes are malformed or conflict",
                range,
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
        if visibility_from_extensions(self.context.source, extensions).is_err() {
            self.draft(id).support = unsupported_status(
                "PARC-E1215",
                "visibility attributes are malformed, unknown, or conflict",
            );
            let diagnostic = self.diagnostic(
                "PARC-E1215",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "visibility attributes are malformed, unknown, or conflict",
                range,
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
        if has_unmodeled_extensions(extensions) {
            if matches!(self.draft(id).support, SupportStatus::Supported) {
                self.draft(id).support = partial_status(
                    "PARC-P1205",
                    "declaration contains an unmodeled ABI-relevant attribute",
                );
            }
            let diagnostic = self.diagnostic(
                "PARC-P1205",
                DiagnosticStage::Extract,
                Severity::Warning,
                DiagnosticCompletenessImpact::ForcesPartial,
                "declaration contains an unmodeled ABI-relevant attribute",
                range,
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
    }

    fn apply_specifier_support(
        &mut self,
        id: DeclarationId,
        specifiers: &[Node<DeclarationSpecifier>],
    ) {
        for specifier in specifiers {
            let (code_value, message) = match &specifier.node {
                DeclarationSpecifier::Alignment(_) => (
                    "PARC-E1216",
                    "explicit declaration alignment is ABI-relevant and not modeled",
                ),
                DeclarationSpecifier::Function(_) => (
                    "PARC-E1217",
                    "function specifier semantics are not modeled by source-contract lowering",
                ),
                _ => continue,
            };
            self.draft(id).support = unsupported_status(code_value, message);
            let diagnostic = self.diagnostic(
                code_value,
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                message,
                self.range(specifier.span),
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }
    }

    fn draft(&mut self, id: DeclarationId) -> &mut DeclarationDraft {
        let meta = self.metadata[&id].clone();
        self.drafts.entry(id).or_insert(DeclarationDraft {
            meta,
            visibility: self.context.default_visibility,
            occurrences: Vec::new(),
            support: SupportStatus::Supported,
            kind: None,
        })
    }

    fn push_unbound_declarator_diagnostic(&mut self, span: Span, message: impl Into<String>) {
        let diagnostic = self.diagnostic(
            "PARC-E1213",
            DiagnosticStage::Extract,
            Severity::Error,
            DiagnosticCompletenessImpact::ForcesRejected,
            message,
            self.range(span),
            None,
        );
        self.diagnostics.push(diagnostic);
    }

    fn range(&self, span: Span) -> Option<SourceRange> {
        if span.is_none() || span.start > span.end || span.end > self.context.source.len() {
            return None;
        }
        Some(SourceRange {
            file: self.context.generated_file,
            start: u64::try_from(span.start).ok()?,
            end: u64::try_from(span.end).ok()?,
        })
    }

    fn text(&self, span: Span) -> Option<String> {
        valid_span(span, self.context.source.len())
            .then(|| self.context.source[span.start..span.end].to_owned())
    }

    fn tokens(&self, span: Span) -> Option<Vec<String>> {
        if !valid_span(span, self.context.source.len()) {
            return None;
        }
        Some(
            Lexer::tokenize(&self.context.source[span.start..span.end])
                .into_iter()
                .filter(|token| {
                    !matches!(
                        token.kind,
                        TokenKind::Whitespace
                            | TokenKind::Newline
                            | TokenKind::LineComment
                            | TokenKind::BlockComment
                            | TokenKind::Eof
                    )
                })
                .map(|token| token.text)
                .collect(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn diagnostic(
        &self,
        code_value: &str,
        stage: DiagnosticStage,
        severity: Severity,
        completeness_impact: DiagnosticCompletenessImpact,
        message: impl Into<String>,
        range: Option<SourceRange>,
        declaration: Option<DeclarationId>,
    ) -> SourceDiagnostic {
        SourceDiagnostic {
            code: code(code_value),
            stage,
            severity,
            completeness_impact,
            message: message.into(),
            range,
            related: Vec::new(),
            declaration,
            target: self.context.target,
        }
    }

    fn finish(mut self) -> ExtractionOutput {
        let missing_occurrences = self
            .metadata
            .keys()
            .copied()
            .filter(|id| {
                self.drafts
                    .get(id)
                    .is_none_or(|draft| draft.occurrences.is_empty())
            })
            .collect::<Vec<_>>();
        for _id in missing_occurrences {
            let diagnostic = self.diagnostic(
                "PARC-E1208",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "an indexed declaration had no contract-safe source occurrence",
                None,
                None,
            );
            self.diagnostics.push(diagnostic);
        }

        let missing_kinds = self
            .drafts
            .iter()
            .filter(|(_, draft)| draft.kind.is_none() && !draft.occurrences.is_empty())
            .map(|(id, draft)| {
                (
                    *id,
                    draft.occurrences[0].range,
                    draft.occurrences[0].spelling.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (id, range, spelling) in missing_kinds {
            let draft = self.drafts.get_mut(&id).expect("draft was just indexed");
            draft.kind = Some(SourceDeclarationKind::Unsupported(SourceUnsupported {
                category: UnsupportedDeclarationCategory::InvalidDeclaration,
                spelling,
                diagnostic: code("PARC-E1209"),
            }));
            draft.support = unsupported_status(
                "PARC-E1209",
                "the declaration kind could not be lowered without guessing",
            );
            let diagnostic = self.diagnostic(
                "PARC-E1209",
                DiagnosticStage::Extract,
                Severity::Error,
                DiagnosticCompletenessImpact::ForcesRejected,
                "the declaration kind could not be lowered without guessing",
                Some(range),
                Some(id),
            );
            self.diagnostics.push(diagnostic);
        }

        let unsupported_statuses = self
            .drafts
            .iter()
            .filter_map(|(id, draft)| {
                let (code, reason, severity, impact) = match &draft.support {
                    SupportStatus::Supported => return None,
                    SupportStatus::Partial { code, reason } => (
                        code.clone(),
                        reason.clone(),
                        Severity::Warning,
                        DiagnosticCompletenessImpact::ForcesPartial,
                    ),
                    SupportStatus::Unsupported { code, reason } => (
                        code.clone(),
                        reason.clone(),
                        Severity::Error,
                        DiagnosticCompletenessImpact::ForcesRejected,
                    ),
                };
                Some((
                    *id,
                    draft.occurrences.last().map(|occurrence| occurrence.range),
                    code,
                    reason,
                    severity,
                    impact,
                ))
            })
            .collect::<Vec<_>>();
        for (id, range, status_code, reason, severity, impact) in unsupported_statuses {
            let already_reported = self.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == status_code && diagnostic.declaration == Some(id)
            });
            if !already_reported {
                let diagnostic = self.diagnostic(
                    status_code.as_str(),
                    DiagnosticStage::Extract,
                    severity,
                    impact,
                    reason,
                    range,
                    Some(id),
                );
                self.diagnostics.push(diagnostic);
            }
        }

        let mut declarations = Vec::new();
        for (id, mut draft) in self.drafts {
            if draft.occurrences.is_empty() {
                continue;
            }
            draft.occurrences.sort_by_key(|occurrence| occurrence.id);
            let kind = draft
                .kind
                .expect("missing kinds are converted to unsupported declarations");
            declarations.push(SourceDeclaration {
                id,
                identity: draft.meta.identity,
                name: draft.meta.name,
                linkage: draft.meta.linkage,
                visibility: draft.visibility,
                occurrences: draft.occurrences,
                support: draft.support,
                kind,
            });
        }
        declarations.sort_by_key(|declaration| declaration.id);
        self.diagnostics.sort();
        self.diagnostics.dedup();
        ExtractionOutput {
            declarations,
            diagnostics: self.diagnostics,
        }
    }
}

fn has_storage(specifiers: &[Node<DeclarationSpecifier>], expected: StorageClassSpecifier) -> bool {
    specifiers.iter().any(|specifier| {
        matches!(
            &specifier.node,
            DeclarationSpecifier::StorageClass(storage) if storage.node == expected
        )
    })
}

fn declaration_storage(specifiers: &[Node<DeclarationSpecifier>]) -> StorageClass {
    specifiers
        .iter()
        .find_map(|specifier| match &specifier.node {
            DeclarationSpecifier::StorageClass(storage) => Some(match storage.node {
                StorageClassSpecifier::Typedef => StorageClass::Typedef,
                StorageClassSpecifier::Extern => StorageClass::Extern,
                StorageClassSpecifier::Static => StorageClass::Static,
                StorageClassSpecifier::ThreadLocal => StorageClass::ThreadLocal,
                StorageClassSpecifier::Auto => StorageClass::Auto,
                StorageClassSpecifier::Register => StorageClass::Register,
                StorageClassSpecifier::Constexpr => StorageClass::None,
            }),
            _ => None,
        })
        .unwrap_or(StorageClass::None)
}

fn declaration_is_definition(
    is_typedef: bool,
    is_function: bool,
    storage: StorageClass,
    has_initializer: bool,
) -> bool {
    if is_typedef {
        return true;
    }
    if is_function {
        return false;
    }
    has_initializer || storage != StorageClass::Extern
}

fn increment_exact_integer(value: ExactInteger) -> Option<ExactInteger> {
    match value {
        ExactInteger::Signed { value } => value.checked_add(1).map(ExactInteger::signed),
        ExactInteger::Unsigned { value } => value.checked_add(1).map(ExactInteger::unsigned),
    }
}

fn redeclaration_action(
    existing: &SourceDeclarationKind,
    replacement: &SourceDeclarationKind,
) -> RedeclarationAction {
    match (existing, replacement) {
        (SourceDeclarationKind::Function(left), SourceDeclarationKind::Function(right)) => {
            if function_signatures_match(left, right) {
                RedeclarationAction::Keep
            } else {
                RedeclarationAction::Conflict
            }
        }
        (SourceDeclarationKind::Variable(left), SourceDeclarationKind::Variable(right)) => {
            if left == right {
                RedeclarationAction::Keep
            } else {
                RedeclarationAction::Conflict
            }
        }
        (SourceDeclarationKind::TypeAlias(left), SourceDeclarationKind::TypeAlias(right)) => {
            if left == right {
                RedeclarationAction::Keep
            } else {
                RedeclarationAction::Conflict
            }
        }
        (SourceDeclarationKind::Record(left), SourceDeclarationKind::Record(right)) => {
            if left.kind != right.kind {
                return RedeclarationAction::Conflict;
            }
            match (left.completeness, right.completeness) {
                (RecordCompleteness::Incomplete, RecordCompleteness::Incomplete)
                | (RecordCompleteness::Complete, RecordCompleteness::Incomplete) => {
                    RedeclarationAction::Keep
                }
                (RecordCompleteness::Incomplete, RecordCompleteness::Complete) => {
                    RedeclarationAction::Replace
                }
                (RecordCompleteness::Complete, RecordCompleteness::Complete) => {
                    RedeclarationAction::Conflict
                }
            }
        }
        (SourceDeclarationKind::Enum(left), SourceDeclarationKind::Enum(right)) => {
            if left.explicit_underlying_type != right.explicit_underlying_type {
                return RedeclarationAction::Conflict;
            }
            match (left.variants.is_empty(), right.variants.is_empty()) {
                (true, true) | (false, true) => RedeclarationAction::Keep,
                (true, false) => RedeclarationAction::Replace,
                (false, false) => RedeclarationAction::Conflict,
            }
        }
        (SourceDeclarationKind::Unsupported(left), SourceDeclarationKind::Unsupported(right)) => {
            if left == right {
                RedeclarationAction::Keep
            } else {
                RedeclarationAction::Conflict
            }
        }
        _ => RedeclarationAction::Conflict,
    }
}

fn function_signatures_match(left: &SourceFunction, right: &SourceFunction) -> bool {
    left.link_name == right.link_name
        && left.return_type == right.return_type
        && left.prototype == right.prototype
        && left.calling_convention == right.calling_convention
        && left.parameters.len() == right.parameters.len()
        && left
            .parameters
            .iter()
            .zip(&right.parameters)
            .all(|(left, right)| left.ty == right.ty)
}

fn type_is_effectively_pointer(ty: &CType, pointer_aliases: &BTreeSet<DeclarationId>) -> bool {
    match &ty.kind {
        CTypeKind::Pointer(_) => true,
        CTypeKind::AliasRef(id) => pointer_aliases.contains(id),
        _ => false,
    }
}

fn declaration_extensions(
    specifiers: &[Node<DeclarationSpecifier>],
    declarator: &Declarator,
) -> Vec<Node<Extension>> {
    let mut extensions = Vec::new();
    for specifier in specifiers {
        if let DeclarationSpecifier::Extension(values) = &specifier.node {
            extensions.extend(values.iter().cloned());
        }
    }
    collect_declarator_extensions(declarator, &mut extensions);
    extensions
}

fn field_extensions(
    specifiers: &[Node<SpecifierQualifier>],
    declarator: Option<&Declarator>,
) -> Vec<Node<Extension>> {
    let mut extensions = Vec::new();
    for specifier in specifiers {
        if let SpecifierQualifier::Extension(values) = &specifier.node {
            extensions.extend(values.iter().cloned());
        }
    }
    if let Some(declarator) = declarator {
        collect_declarator_extensions(declarator, &mut extensions);
    }
    extensions
}

fn collect_declarator_extensions(declarator: &Declarator, output: &mut Vec<Node<Extension>>) {
    output.extend(declarator.extensions.iter().cloned());
    for derived in &declarator.derived {
        if let DerivedDeclarator::Pointer(qualifiers) | DerivedDeclarator::Block(qualifiers) =
            &derived.node
        {
            for qualifier in qualifiers {
                if let PointerQualifier::Extension(values) = &qualifier.node {
                    output.extend(values.iter().cloned());
                }
            }
        }
    }
    if let DeclaratorKind::Declarator(inner) = &declarator.kind.node {
        collect_declarator_extensions(&inner.node, output);
    }
}

fn modeled_calling_convention(attribute: &Attribute) -> Result<Option<CallingConvention>, ()> {
    let name = attribute.name.node.trim().to_ascii_lowercase();
    let convention = match name.as_str() {
        "cdecl" | "__cdecl" => Some(CallingConvention::Cdecl),
        "stdcall" | "__stdcall" => Some(CallingConvention::Stdcall),
        "fastcall" | "__fastcall" => Some(CallingConvention::Fastcall),
        "vectorcall" | "__vectorcall" => Some(CallingConvention::Vectorcall),
        "thiscall" | "__thiscall" => Some(CallingConvention::Thiscall),
        "sysv_abi" => Some(CallingConvention::SysV64),
        "ms_abi" => Some(CallingConvention::Win64),
        "aapcs" => Some(CallingConvention::Aapcs),
        "pcs" => {
            let [argument] = attribute.arguments.as_slice() else {
                return Err(());
            };
            let Expression::StringLiteral(literal) = &argument.node else {
                return Err(());
            };
            return match decode_simple_c_string(&literal.node).as_deref() {
                Some("aapcs") => Ok(Some(CallingConvention::Aapcs)),
                _ => Err(()),
            };
        }
        _ => None,
    };
    if convention.is_some() && !attribute.arguments.is_empty() {
        return Err(());
    }
    Ok(convention)
}

fn calling_convention(extensions: &[Node<Extension>]) -> Result<CallingConvention, String> {
    let mut found = None;
    let mut spellings = Vec::new();
    for extension in extensions {
        let convention = match &extension.node {
            Extension::Attribute(attribute) => match modeled_calling_convention(attribute) {
                Ok(convention) => convention,
                Err(()) => return Err(extension_attribute_spelling(extension)),
            },
            _ => None,
        };
        let Some(convention) = convention else {
            continue;
        };
        spellings.push(extension_attribute_spelling(extension));
        if found
            .as_ref()
            .is_some_and(|existing| existing != &convention)
        {
            return Err(spellings.join(" "));
        }
        found = Some(convention);
    }
    Ok(found.unwrap_or(CallingConvention::C))
}

fn extension_attribute_spelling(extension: &Node<Extension>) -> String {
    match &extension.node {
        Extension::Attribute(attribute) => attribute.name.node.clone(),
        Extension::AsmLabel(parts) => parts.node.join(""),
        Extension::AvailabilityAttribute(_) => "availability".to_owned(),
    }
}

fn asm_link_name(extensions: &[Node<Extension>]) -> Result<Option<String>, String> {
    let Some(parts) = extensions
        .iter()
        .find_map(|extension| match &extension.node {
            Extension::AsmLabel(parts) => Some(&parts.node),
            _ => None,
        })
    else {
        return Ok(None);
    };

    let spelling = parts.join("");
    let Some(decoded) = decode_simple_c_string(parts) else {
        return Err(spelling);
    };
    if decoded.is_empty() {
        Err(spelling)
    } else {
        Ok(Some(decoded))
    }
}

fn decode_simple_c_string(parts: &[String]) -> Option<String> {
    let mut decoded = String::new();
    for part in parts {
        let body = part
            .strip_prefix('"')
            .and_then(|part| part.strip_suffix('"'))?;
        let mut characters = body.chars();
        while let Some(character) = characters.next() {
            if character != '\\' {
                if character == '\0' {
                    return None;
                }
                decoded.push(character);
                continue;
            }
            let escaped = characters.next()?;
            match escaped {
                '\\' | '"' | '\'' | '?' => decoded.push(escaped),
                _ => return None,
            }
        }
    }
    Some(decoded)
}

struct LoweredAttributes {
    attributes: Vec<SourceAttribute>,
    invalid_span: bool,
}

fn attributes_from_extensions(
    source: &str,
    file: FileId,
    extensions: &[Node<Extension>],
) -> LoweredAttributes {
    let mut attributes = Vec::new();
    let mut invalid_span = false;
    for extension in extensions {
        if !valid_span(extension.span, source.len()) {
            invalid_span = true;
            continue;
        }
        let (Ok(start), Ok(end)) = (
            u64::try_from(extension.span.start),
            u64::try_from(extension.span.end),
        ) else {
            invalid_span = true;
            continue;
        };
        let range = SourceRange { file, start, end };
        let spelling = source[extension.span.start..extension.span.end].to_owned();
        let (name, arguments, disposition) = match &extension.node {
            Extension::Attribute(attribute) => {
                let mut arguments = Vec::new();
                let mut valid_arguments = true;
                for argument in &attribute.arguments {
                    if !valid_span(argument.span, source.len()) {
                        invalid_span = true;
                        valid_arguments = false;
                        break;
                    }
                    arguments.push(source[argument.span.start..argument.span.end].to_owned());
                }
                if !valid_arguments {
                    continue;
                }
                let name = attribute.name.node.clone();
                let disposition = if is_modeled_attribute(&name) {
                    AttributeDisposition::Modeled
                } else {
                    AttributeDisposition::UnsupportedAbiRelevant
                };
                (name, arguments, disposition)
            }
            Extension::AsmLabel(_) => (
                "asm_label".to_owned(),
                Vec::new(),
                AttributeDisposition::Modeled,
            ),
            Extension::AvailabilityAttribute(_) => (
                "availability".to_owned(),
                Vec::new(),
                AttributeDisposition::Preserved,
            ),
        };
        attributes.push(SourceAttribute {
            namespace: None,
            name,
            arguments,
            spelling,
            range,
            disposition,
        });
    }
    LoweredAttributes {
        attributes,
        invalid_span,
    }
}

fn valid_span(span: Span, source_len: usize) -> bool {
    !span.is_none() && span.start <= span.end && span.end <= source_len
}

fn generated_range_text(source: &str, range: SourceRange) -> Option<String> {
    let start = usize::try_from(range.start).ok()?;
    let end = usize::try_from(range.end).ok()?;
    (start <= end && end <= source.len()).then(|| source[start..end].to_owned())
}

fn is_modeled_attribute(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "cdecl"
            | "__cdecl"
            | "stdcall"
            | "__stdcall"
            | "fastcall"
            | "__fastcall"
            | "vectorcall"
            | "__vectorcall"
            | "thiscall"
            | "__thiscall"
            | "sysv_abi"
            | "ms_abi"
            | "pcs"
            | "aapcs"
            | "visibility"
    )
}

fn has_unmodeled_extensions(extensions: &[Node<Extension>]) -> bool {
    extensions.iter().any(|extension| match &extension.node {
        Extension::Attribute(attribute) => !is_modeled_attribute(&attribute.name.node),
        Extension::AsmLabel(_) | Extension::AvailabilityAttribute(_) => false,
    })
}

fn visibility_from_extensions(
    source: &str,
    extensions: &[Node<Extension>],
) -> Result<Option<Visibility>, String> {
    let mut found = None;
    let mut spellings = Vec::new();
    for extension in extensions {
        let Extension::Attribute(attribute) = &extension.node else {
            continue;
        };
        if !attribute
            .name
            .node
            .trim()
            .eq_ignore_ascii_case("visibility")
        {
            continue;
        }
        let spelling = extension_source_spelling(source, extension);
        spellings.push(spelling.clone());
        let [argument] = attribute.arguments.as_slice() else {
            return Err(spelling);
        };
        let Expression::StringLiteral(literal) = &argument.node else {
            return Err(spelling);
        };
        let Some(value) = decode_simple_c_string(&literal.node) else {
            return Err(spelling);
        };
        let visibility = match value.as_str() {
            "default" => Visibility::ExplicitDefault,
            "hidden" => Visibility::Hidden,
            "protected" => Visibility::Protected,
            "internal" => Visibility::Internal,
            _ => return Err(spelling),
        };
        if found
            .as_ref()
            .is_some_and(|existing| existing != &visibility)
        {
            return Err(spellings.join(" "));
        }
        found = Some(visibility);
    }
    Ok(found)
}

fn extension_source_spelling(source: &str, extension: &Node<Extension>) -> String {
    if valid_span(extension.span, source.len()) {
        source[extension.span.start..extension.span.end].to_owned()
    } else {
        extension_attribute_spelling(extension)
    }
}

fn generated_provenance() -> SourceProvenance {
    SourceProvenance {
        origin: SourceOrigin::Generated,
        include_chain: Vec::new(),
        macro_expansions: Vec::new(),
    }
}

fn declaration_kind_support(kind: &SourceDeclarationKind) -> Option<SupportStatus> {
    match kind {
        SourceDeclarationKind::Function(function) => nested_type_support(&function.return_type)
            .or_else(|| {
                function.parameters.iter().find_map(|parameter| {
                    (!parameter.support.is_supported())
                        .then(|| parameter.support.clone())
                        .or_else(|| nested_type_support(&parameter.ty))
                })
            })
            .or_else(|| match &function.calling_convention {
                CallingConvention::Unsupported { spelling } => Some(unsupported_status(
                    "PARC-E1206",
                    &format!("unsupported calling convention: {spelling}"),
                )),
                _ => None,
            }),
        SourceDeclarationKind::Record(record) => record.fields.iter().find_map(|field| {
            (!field.support.is_supported())
                .then(|| field.support.clone())
                .or_else(|| nested_type_support(&field.ty))
        }),
        SourceDeclarationKind::Enum(enumeration) => enumeration
            .variants
            .iter()
            .find_map(|variant| (!variant.support.is_supported()).then(|| variant.support.clone()))
            .or_else(|| {
                enumeration
                    .explicit_underlying_type
                    .as_ref()
                    .and_then(nested_type_support)
            }),
        SourceDeclarationKind::TypeAlias(alias) => nested_type_support(&alias.target),
        SourceDeclarationKind::Variable(variable) => nested_type_support(&variable.ty),
        SourceDeclarationKind::Unsupported(unsupported) => Some(unsupported_status(
            unsupported.diagnostic.as_str(),
            &unsupported.spelling,
        )),
    }
}

fn nested_type_support(ty: &CType) -> Option<SupportStatus> {
    if !ty.support.is_supported() {
        return Some(ty.support.clone());
    }
    match &ty.kind {
        CTypeKind::Pointer(inner) => nested_type_support(inner),
        CTypeKind::Array { element, .. } => nested_type_support(element),
        CTypeKind::Function(function) => nested_type_support(&function.return_type).or_else(|| {
            function
                .parameters
                .iter()
                .find_map(|parameter| nested_type_support(&parameter.ty))
        }),
        _ => None,
    }
}

fn partial_status(code_value: &str, reason: &str) -> SupportStatus {
    SupportStatus::Partial {
        code: code(code_value),
        reason: reason.to_owned(),
    }
}

fn unsupported_status(code_value: &str, reason: &str) -> SupportStatus {
    SupportStatus::Unsupported {
        code: code(code_value),
        reason: reason.to_owned(),
    }
}
