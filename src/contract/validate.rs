#![allow(clippy::needless_borrows_for_generic_args)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{
    ids::{canonical_tokens_bytes, normalize_identifier, normalize_logical_path},
    ArrayBound, ArrayMinimumBound, BitIntWidth, CType, CTypeKind, CallingConvention,
    CapturedEnvironment, ChildId, ChildRole, Completeness, DeclarationId, DeclarationIdentity,
    DefineEvent, DiagnosticCompletenessImpact, DiagnosticStage, EntityId, EntityNamespace,
    EntityScope, EnvironmentInputs, FileId, FunctionPrototype, Linkage, MacroId, Nullability,
    OccurrenceId, PreprocessorIdentity, RecordCompleteness, RecordKind, Severity, SourceAttribute,
    SourceDeclaration, SourceDeclarationKind, SourceFile, SourceFileRole, SourceOrigin,
    SourcePackage, SourceProvenance, SourceRange, TypeQualifiers,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractViolationCode {
    Schema,
    NonCanonicalOrder,
    DuplicateId,
    InvalidFile,
    InvalidRange,
    InvalidId,
    InvalidName,
    InvalidReference,
    WrongReferenceKind,
    AliasCycle,
    InvalidType,
    InvalidCompleteness,
    InvalidTarget,
    LimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractViolation {
    pub code: ContractViolationCode,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ValidationLimits {
    pub files: usize,
    pub declarations: usize,
    pub macros: usize,
    pub diagnostics: usize,
    pub type_depth: usize,
}

pub(crate) fn validate_source_package(
    package: &SourcePackage,
    limits: ValidationLimits,
) -> Result<(), Vec<ContractViolation>> {
    let mut validator = Validator {
        package,
        limits,
        violations: Vec::new(),
        files: BTreeMap::new(),
        declarations: BTreeMap::new(),
        child_ids: BTreeSet::new(),
        occurrence_ids: BTreeSet::new(),
    };
    validator.validate();
    if validator.violations.is_empty() {
        Ok(())
    } else {
        validator
            .violations
            .sort_by(|left, right| (&left.path, left.code).cmp(&(&right.path, right.code)));
        Err(validator.violations)
    }
}

struct Validator<'a> {
    package: &'a SourcePackage,
    limits: ValidationLimits,
    violations: Vec<ContractViolation>,
    files: BTreeMap<FileId, &'a SourceFile>,
    declarations: BTreeMap<DeclarationId, &'a SourceDeclaration>,
    child_ids: BTreeSet<ChildId>,
    occurrence_ids: BTreeSet<OccurrenceId>,
}

impl Validator<'_> {
    fn validate(&mut self) {
        if !self.package.schema().is_source_package_v2() {
            self.push(
                ContractViolationCode::Schema,
                "schema",
                "expected the exact PARC source-package schema v2 header",
            );
        }
        self.check_limit("files", self.package.files().len(), self.limits.files);
        self.check_limit(
            "declarations",
            self.package.declarations().len(),
            self.limits.declarations,
        );
        self.check_limit("macros", self.package.macros().len(), self.limits.macros);
        self.check_limit(
            "diagnostics",
            self.package.diagnostics().len(),
            self.limits.diagnostics,
        );

        if let Err(error) = self.package.target().validate() {
            for (index, violation) in error.violations().iter().enumerate() {
                self.push(
                    ContractViolationCode::InvalidTarget,
                    format!("target.violations[{index}]"),
                    violation.to_string(),
                );
            }
        }

        self.validate_files();
        self.validate_declaration_table();
        self.validate_declarations();
        self.validate_macros();
        self.validate_diagnostics();
        self.validate_inputs();
        self.validate_alias_cycles();
        self.validate_completeness();
    }

    fn check_limit(&mut self, path: &str, actual: usize, maximum: usize) {
        if actual > maximum {
            self.push(
                ContractViolationCode::LimitExceeded,
                path,
                format!("contains {actual} entries; decoder limit is {maximum}"),
            );
        }
    }

    fn validate_files(&mut self) {
        if !strictly_sorted_by_key(self.package.files(), |file| file.id) {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "files",
                "file table must be strictly ordered by FileId",
            );
        }
        for (index, file) in self.package.files().iter().enumerate() {
            let path = format!("files[{index}]");
            if self.files.insert(file.id, file).is_some() {
                self.push(
                    ContractViolationCode::DuplicateId,
                    &format!("{path}.id"),
                    "duplicate FileId",
                );
            }
            match normalize_logical_path(&file.logical_path) {
                Ok(normalized) if normalized == file.logical_path => {}
                _ => self.push(
                    ContractViolationCode::InvalidFile,
                    &format!("{path}.logical_path"),
                    "logical path is not canonical",
                ),
            }
            match FileId::from_logical_path(&file.logical_path) {
                Ok(expected) if expected == file.id => {}
                _ => self.push(
                    ContractViolationCode::InvalidId,
                    &format!("{path}.id"),
                    "FileId does not match logical path",
                ),
            }
            if file.line_starts.first() != Some(&0)
                || file.line_starts.windows(2).any(|pair| pair[0] >= pair[1])
                || file
                    .line_starts
                    .last()
                    .is_some_and(|last| *last > file.byte_len)
            {
                self.push(
                    ContractViolationCode::InvalidFile,
                    &format!("{path}.line_starts"),
                    "line starts must begin at zero, increase, and remain within the file",
                );
            }
        }
    }

    fn validate_declaration_table(&mut self) {
        if !strictly_sorted_by_key(self.package.declarations(), |declaration| declaration.id) {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "declarations",
                "declaration table must be strictly ordered by DeclarationId",
            );
        }
        for (index, declaration) in self.package.declarations().iter().enumerate() {
            if self
                .declarations
                .insert(declaration.id, declaration)
                .is_some()
            {
                self.push(
                    ContractViolationCode::DuplicateId,
                    &format!("declarations[{index}].id"),
                    "duplicate DeclarationId",
                );
            }
        }
    }

    fn validate_declarations(&mut self) {
        for (index, declaration) in self.package.declarations().iter().enumerate() {
            let path = format!("declarations[{index}]");
            self.validate_declaration_identity(declaration, &path);
            if declaration.occurrences.is_empty() {
                self.push(
                    ContractViolationCode::InvalidId,
                    &format!("{path}.occurrences"),
                    "declaration must contain at least one occurrence",
                );
            }
            if !strictly_sorted_by_key(&declaration.occurrences, |occurrence| occurrence.id) {
                self.push(
                    ContractViolationCode::NonCanonicalOrder,
                    &format!("{path}.occurrences"),
                    "occurrences must be strictly ordered by OccurrenceId",
                );
            }
            for (occurrence_index, occurrence) in declaration.occurrences.iter().enumerate() {
                let occurrence_path = format!("{path}.occurrences[{occurrence_index}]");
                self.validate_range(&occurrence.range, &format!("{occurrence_path}.range"));
                if occurrence.range.start == occurrence.range.end {
                    self.push(
                        ContractViolationCode::InvalidRange,
                        &format!("{occurrence_path}.range"),
                        "declaration occurrence range must be nonempty",
                    );
                }
                if occurrence.spelling.is_empty() || occurrence.normalized_tokens.is_empty() {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &occurrence_path,
                        "declaration occurrence spelling and normalized tokens must be nonempty",
                    );
                }
                if let Some(range) = occurrence.name_range {
                    self.validate_range(&range, &format!("{occurrence_path}.name_range"));
                    if range.start == range.end || !source_range_contains(occurrence.range, range) {
                        self.push(
                            ContractViolationCode::InvalidRange,
                            &format!("{occurrence_path}.name_range"),
                            "name range must be nonempty, same-file, and contained in its occurrence",
                        );
                    }
                }
                self.validate_attributes(&occurrence.attributes, &occurrence_path);
                self.validate_provenance(
                    &occurrence.provenance,
                    occurrence.range.file,
                    &occurrence_path,
                );
                if !self.occurrence_ids.insert(occurrence.id) {
                    self.push(
                        ContractViolationCode::DuplicateId,
                        &format!("{occurrence_path}.id"),
                        "duplicate OccurrenceId",
                    );
                }
                let tokens = canonical_tokens_bytes(&occurrence.normalized_tokens);
                let expected = OccurrenceId::derive(
                    declaration.id,
                    occurrence.range.file,
                    &tokens,
                    occurrence.duplicate_ordinal,
                );
                if expected != occurrence.id {
                    self.push(
                        ContractViolationCode::InvalidId,
                        &format!("{occurrence_path}.id"),
                        "OccurrenceId does not match its declaration, file, tokens, and ordinal",
                    );
                }
            }
            self.validate_declaration_kind(declaration, &path);
        }
    }

    fn validate_declaration_identity(&mut self, declaration: &SourceDeclaration, path: &str) {
        let expected = match &declaration.identity {
            DeclarationIdentity::Named {
                namespace,
                scope,
                normalized_name,
            } => {
                let Some(name) = &declaration.name else {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &format!("{path}.name"),
                        "named identity requires a source name",
                    );
                    return;
                };
                if normalize_identifier(&name.normalized).as_deref() != Ok(name.normalized.as_str())
                    || name.normalized != *normalized_name
                    || name.original.is_empty()
                {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &format!("{path}.name"),
                        "name must be canonical and match the identity key",
                    );
                }
                self.validate_entity_scope(*scope, &format!("{path}.identity.scope"));
                match EntityId::named(*namespace, *scope, normalized_name) {
                    Ok(entity) => DeclarationId::from_entity(entity),
                    Err(error) => {
                        self.push(
                            ContractViolationCode::InvalidName,
                            &format!("{path}.identity.normalized_name"),
                            error.to_string(),
                        );
                        return;
                    }
                }
            }
            DeclarationIdentity::Anonymous {
                scope,
                token_fingerprint,
                duplicate_ordinal,
            } => {
                if declaration.name.is_some() {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &format!("{path}.name"),
                        "anonymous identity cannot carry a declaration name",
                    );
                }
                self.validate_entity_scope(*scope, &format!("{path}.identity.scope"));
                DeclarationId::from_entity(EntityId::anonymous(
                    *scope,
                    token_fingerprint.as_bytes(),
                    *duplicate_ordinal,
                ))
            }
        };
        if expected != declaration.id {
            self.push(
                ContractViolationCode::InvalidId,
                &format!("{path}.id"),
                "DeclarationId does not match its identity key",
            );
        }

        let expected_namespace = match declaration.kind {
            SourceDeclarationKind::Record(_) | SourceDeclarationKind::Enum(_) => {
                Some(EntityNamespace::Tag)
            }
            SourceDeclarationKind::Function(_)
            | SourceDeclarationKind::TypeAlias(_)
            | SourceDeclarationKind::Variable(_) => Some(EntityNamespace::Ordinary),
            SourceDeclarationKind::Unsupported(_) => None,
        };
        if let (Some(expected_namespace), DeclarationIdentity::Named { namespace, .. }) =
            (expected_namespace, &declaration.identity)
        {
            if *namespace != expected_namespace {
                self.push(
                    ContractViolationCode::InvalidId,
                    &format!("{path}.identity.namespace"),
                    "identity namespace does not match declaration kind",
                );
            }
        }
        if declaration.linkage == Linkage::Internal
            && !matches!(
                declaration.identity,
                DeclarationIdentity::Named {
                    scope: EntityScope::File(_),
                    ..
                }
            )
        {
            self.push(
                ContractViolationCode::InvalidId,
                &format!("{path}.identity.scope"),
                "internal linkage requires file scope identity",
            );
        }
    }

    fn validate_entity_scope(&mut self, scope: EntityScope, path: &str) {
        match scope {
            EntityScope::TranslationUnit => {}
            EntityScope::File(file) if self.files.contains_key(&file) => {}
            EntityScope::Owner(owner) if self.declarations.contains_key(&owner) => {}
            EntityScope::File(_) | EntityScope::Owner(_) => self.push(
                ContractViolationCode::InvalidReference,
                path,
                "identity scope references an unknown file or owner",
            ),
        }
    }

    fn validate_declaration_kind(&mut self, declaration: &SourceDeclaration, path: &str) {
        match &declaration.kind {
            SourceDeclarationKind::Function(function) => {
                if function.link_name.is_empty() {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &format!("{path}.kind.value.link_name"),
                        "function link name must be nonempty",
                    );
                }
                self.validate_type(
                    &function.return_type,
                    &format!("{path}.kind.value.return_type"),
                    0,
                    TypeContext::General,
                );
                if matches!(function.prototype, FunctionPrototype::UnspecifiedParameters)
                    && !function.parameters.is_empty()
                {
                    self.push(
                        ContractViolationCode::InvalidType,
                        &format!("{path}.kind.value.parameters"),
                        "unspecified-parameter function cannot carry parameter declarations",
                    );
                }
                for (parameter_index, parameter) in function.parameters.iter().enumerate() {
                    let parameter_path = format!("{path}.kind.value.parameters[{parameter_index}]");
                    self.validate_parameter_id(
                        declaration.id,
                        parameter.id,
                        parameter.ordinal,
                        parameter_index,
                        &parameter_path,
                    );
                    self.validate_range(&parameter.range, &format!("{parameter_path}.range"));
                    self.validate_child_range(declaration, parameter.range, &parameter_path);
                    self.validate_provenance(
                        &parameter.provenance,
                        parameter.range.file,
                        &parameter_path,
                    );
                    self.validate_attributes(&parameter.attributes, &parameter_path);
                    self.validate_type(
                        &parameter.ty,
                        &format!("{parameter_path}.ty"),
                        0,
                        TypeContext::Parameter,
                    );
                }
                if matches!(
                    function.calling_convention,
                    CallingConvention::Unsupported { .. }
                ) && declaration.support.is_supported()
                {
                    self.push(
                        ContractViolationCode::InvalidType,
                        &format!("{path}.support"),
                        "unsupported calling convention requires non-supported status",
                    );
                }
            }
            SourceDeclarationKind::Record(record) => {
                if record.completeness == RecordCompleteness::Incomplete
                    && !record.fields.is_empty()
                {
                    self.push(
                        ContractViolationCode::InvalidType,
                        &format!("{path}.kind.value.fields"),
                        "incomplete record cannot contain fields",
                    );
                }
                for (field_index, field) in record.fields.iter().enumerate() {
                    let field_path = format!("{path}.kind.value.fields[{field_index}]");
                    self.validate_child_id(
                        declaration.id,
                        ChildRole::Field,
                        field.id,
                        field.name.as_ref(),
                        &field.identity_tokens,
                        field.duplicate_ordinal,
                        &field_path,
                    );
                    self.validate_range(&field.range, &format!("{field_path}.range"));
                    self.validate_child_range(declaration, field.range, &field_path);
                    self.validate_provenance(&field.provenance, field.range.file, &field_path);
                    self.validate_attributes(&field.attributes, &field_path);
                    self.validate_type(
                        &field.ty,
                        &format!("{field_path}.ty"),
                        0,
                        TypeContext::RecordField {
                            flexible_allowed: record.kind == RecordKind::Struct
                                && record.completeness == RecordCompleteness::Complete
                                && field_index > 0
                                && field_index + 1 == record.fields.len(),
                        },
                    );
                }
            }
            SourceDeclarationKind::Enum(enumeration) => {
                if let Some(underlying) = &enumeration.explicit_underlying_type {
                    self.validate_type(
                        underlying,
                        &format!("{path}.kind.value.explicit_underlying_type"),
                        0,
                        TypeContext::General,
                    );
                }
                for (variant_index, variant) in enumeration.variants.iter().enumerate() {
                    let variant_path = format!("{path}.kind.value.variants[{variant_index}]");
                    self.validate_child_id(
                        declaration.id,
                        ChildRole::EnumVariant,
                        variant.id,
                        Some(&variant.name),
                        &variant.identity_tokens,
                        variant.duplicate_ordinal,
                        &variant_path,
                    );
                    self.validate_range(&variant.range, &format!("{variant_path}.range"));
                    self.validate_child_range(declaration, variant.range, &variant_path);
                    self.validate_provenance(
                        &variant.provenance,
                        variant.range.file,
                        &variant_path,
                    );
                    self.validate_attributes(&variant.attributes, &variant_path);
                }
            }
            SourceDeclarationKind::TypeAlias(alias) => {
                self.validate_type(
                    &alias.target,
                    &format!("{path}.kind.value.target"),
                    0,
                    TypeContext::General,
                );
            }
            SourceDeclarationKind::Variable(variable) => {
                if variable.link_name.is_empty() {
                    self.push(
                        ContractViolationCode::InvalidName,
                        &format!("{path}.kind.value.link_name"),
                        "variable link name must be nonempty",
                    );
                }
                self.validate_type(
                    &variable.ty,
                    &format!("{path}.kind.value.ty"),
                    0,
                    TypeContext::General,
                );
            }
            SourceDeclarationKind::Unsupported(_) if declaration.support.is_supported() => {
                self.push(
                    ContractViolationCode::InvalidType,
                    &format!("{path}.support"),
                    "unsupported declaration cannot have supported status",
                );
            }
            SourceDeclarationKind::Unsupported(_) => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_child_id(
        &mut self,
        parent: DeclarationId,
        role: ChildRole,
        id: ChildId,
        name: Option<&super::SourceName>,
        identity_tokens: &[String],
        duplicate_ordinal: u64,
        path: &str,
    ) {
        if !self.child_ids.insert(id) {
            self.push(
                ContractViolationCode::DuplicateId,
                &format!("{path}.id"),
                "duplicate ChildId",
            );
        }
        let expected = if let Some(name) = name {
            if name.original.is_empty()
                || normalize_identifier(&name.normalized).as_deref() != Ok(name.normalized.as_str())
            {
                self.push(
                    ContractViolationCode::InvalidName,
                    &format!("{path}.name"),
                    "child name is not canonical",
                );
            }
            ChildId::named(parent, role, &name.normalized).ok()
        } else {
            Some(ChildId::anonymous(
                parent,
                role,
                &canonical_tokens_bytes(identity_tokens),
                duplicate_ordinal,
            ))
        };
        if expected != Some(id) {
            self.push(
                ContractViolationCode::InvalidId,
                &format!("{path}.id"),
                "ChildId does not match its parent, role, name/tokens, and ordinal",
            );
        }
    }

    fn validate_parameter_id(
        &mut self,
        parent: DeclarationId,
        id: ChildId,
        ordinal: u64,
        parameter_index: usize,
        path: &str,
    ) {
        if !self.child_ids.insert(id) {
            self.push(
                ContractViolationCode::DuplicateId,
                &format!("{path}.id"),
                "duplicate ChildId",
            );
        }
        if usize::try_from(ordinal) != Ok(parameter_index) {
            self.push(
                ContractViolationCode::InvalidId,
                &format!("{path}.ordinal"),
                "parameter ordinal must equal its zero-based prototype position",
            );
        }
        if ChildId::parameter(parent, ordinal) != id {
            self.push(
                ContractViolationCode::InvalidId,
                &format!("{path}.id"),
                "parameter ChildId does not match its parent and semantic ordinal",
            );
        }
    }

    fn validate_child_range(
        &mut self,
        declaration: &SourceDeclaration,
        child: SourceRange,
        path: &str,
    ) {
        if child.start == child.end
            || !declaration
                .occurrences
                .iter()
                .any(|occurrence| source_range_contains(occurrence.range, child))
        {
            self.push(
                ContractViolationCode::InvalidRange,
                format!("{path}.range"),
                "child range must be nonempty and contained in an owning declaration occurrence",
            );
        }
    }

    fn validate_type(&mut self, ty: &CType, path: &str, depth: usize, context: TypeContext) {
        if depth > self.limits.type_depth {
            self.push(
                ContractViolationCode::LimitExceeded,
                path,
                "C type nesting exceeds decoder limit",
            );
            return;
        }
        let effectively_pointer = self.type_is_effectively_pointer(ty);
        if ty.nullability != Nullability::Unspecified && !effectively_pointer {
            self.push(
                ContractViolationCode::InvalidType,
                &format!("{path}.nullability"),
                "nullability is only valid on pointer types",
            );
        }
        if ty.qualifiers.is_restrict && !effectively_pointer {
            self.push(
                ContractViolationCode::InvalidType,
                &format!("{path}.qualifiers.is_restrict"),
                "restrict is only valid on pointer types in the contract",
            );
        }
        if matches!(ty.kind, CTypeKind::Unsupported { .. }) && ty.support.is_supported() {
            self.push(
                ContractViolationCode::InvalidType,
                &format!("{path}.support"),
                "unsupported type node cannot have supported status",
            );
        }
        match &ty.kind {
            CTypeKind::Pointer(inner) => self.validate_type(
                inner,
                &format!("{path}.kind.value"),
                depth + 1,
                TypeContext::General,
            ),
            CTypeKind::Array {
                element,
                bound,
                parameter_qualifiers,
            } => {
                self.validate_type(
                    element,
                    &format!("{path}.kind.value.element"),
                    depth + 1,
                    TypeContext::General,
                );
                self.validate_array_bound(bound, *parameter_qualifiers, context, path);
            }
            CTypeKind::Function(function) => {
                self.validate_type(
                    &function.return_type,
                    &format!("{path}.kind.value.return_type"),
                    depth + 1,
                    TypeContext::General,
                );
                for (index, parameter) in function.parameters.iter().enumerate() {
                    self.validate_type(
                        &parameter.ty,
                        &format!("{path}.kind.value.parameters[{index}].ty"),
                        depth + 1,
                        TypeContext::Parameter,
                    );
                }
            }
            CTypeKind::AliasRef(id) => self.validate_reference(*id, ReferenceKind::Alias, path),
            CTypeKind::RecordRef(id) => self.validate_reference(*id, ReferenceKind::Record, path),
            CTypeKind::EnumRef(id) => self.validate_reference(*id, ReferenceKind::Enum, path),
            CTypeKind::Integer(super::CIntegerType::BitInt {
                width: BitIntWidth::Known { bits: 0 },
                ..
            }) => self.push(
                ContractViolationCode::InvalidType,
                path,
                "_BitInt width must be positive",
            ),
            CTypeKind::Void
            | CTypeKind::Bool
            | CTypeKind::Integer(_)
            | CTypeKind::Floating(_)
            | CTypeKind::Complex(_)
            | CTypeKind::Unsupported { .. } => {}
        }
    }

    fn validate_array_bound(
        &mut self,
        bound: &ArrayBound,
        parameter_qualifiers: TypeQualifiers,
        context: TypeContext,
        path: &str,
    ) {
        let bound_path = format!("{path}.kind.value.bound");
        if parameter_qualifiers != TypeQualifiers::NONE && context != TypeContext::Parameter {
            self.push(
                ContractViolationCode::InvalidType,
                format!("{path}.kind.value.parameter_qualifiers"),
                "array-parameter qualifiers are only valid on a parameter's outer array declarator",
            );
        }
        match bound {
            ArrayBound::Fixed { elements: 0 } => self.push(
                ContractViolationCode::InvalidType,
                &bound_path,
                "zero array bounds must be represented as invalid or unsupported",
            ),
            ArrayBound::Variable {
                normalized_expression,
            } if normalized_expression.is_empty() => self.push(
                ContractViolationCode::InvalidType,
                &bound_path,
                "variable array bound expression must be nonempty",
            ),
            ArrayBound::StaticMinimum { minimum } => {
                if context != TypeContext::Parameter {
                    self.push(
                        ContractViolationCode::InvalidType,
                        &bound_path,
                        "static minimum bounds are only valid in parameter array declarators",
                    );
                }
                match minimum {
                    ArrayMinimumBound::Fixed { elements: 0 } => self.push(
                        ContractViolationCode::InvalidType,
                        &bound_path,
                        "zero static minimum must be represented as invalid",
                    ),
                    ArrayMinimumBound::Variable {
                        normalized_expression,
                    } if normalized_expression.is_empty() => self.push(
                        ContractViolationCode::InvalidType,
                        &bound_path,
                        "variable static minimum expression must be nonempty",
                    ),
                    ArrayMinimumBound::Fixed { .. } | ArrayMinimumBound::Variable { .. } => {}
                }
            }
            ArrayBound::Flexible if !context.flexible_allowed() => self.push(
                ContractViolationCode::InvalidType,
                &bound_path,
                "flexible array bounds are only valid on a final struct field",
            ),
            ArrayBound::Incomplete if context.is_record_field() => self.push(
                ContractViolationCode::InvalidType,
                &bound_path,
                "an unsized record field must be represented as flexible, not incomplete",
            ),
            ArrayBound::Invalid { spelling, .. } if spelling.is_empty() => self.push(
                ContractViolationCode::InvalidType,
                &bound_path,
                "invalid array bound must retain its source spelling",
            ),
            ArrayBound::Fixed { .. }
            | ArrayBound::Incomplete
            | ArrayBound::Flexible
            | ArrayBound::Variable { .. }
            | ArrayBound::Invalid { .. } => {}
        }
    }

    fn type_is_effectively_pointer(&self, ty: &CType) -> bool {
        let mut kind = &ty.kind;
        let mut visited = BTreeSet::new();
        loop {
            match kind {
                CTypeKind::Pointer(_) => return true,
                CTypeKind::AliasRef(id) if visited.insert(*id) => {
                    let Some(declaration) = self.declarations.get(id).copied() else {
                        return false;
                    };
                    let SourceDeclarationKind::TypeAlias(alias) = &declaration.kind else {
                        return false;
                    };
                    kind = &alias.target.kind;
                }
                _ => return false,
            }
        }
    }

    fn validate_reference(&mut self, id: DeclarationId, kind: ReferenceKind, path: &str) {
        let Some(declaration) = self.declarations.get(&id) else {
            self.push(
                ContractViolationCode::InvalidReference,
                path,
                format!("references unknown declaration {id}"),
            );
            return;
        };
        let correct = matches!(
            (kind, &declaration.kind),
            (ReferenceKind::Alias, SourceDeclarationKind::TypeAlias(_))
                | (ReferenceKind::Record, SourceDeclarationKind::Record(_))
                | (ReferenceKind::Enum, SourceDeclarationKind::Enum(_))
        );
        if !correct {
            self.push(
                ContractViolationCode::WrongReferenceKind,
                path,
                format!("declaration {id} has the wrong kind for this reference"),
            );
        }
    }

    fn validate_macros(&mut self) {
        if !strictly_sorted_by_key(self.package.macros(), |macro_item| macro_item.id) {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "macros",
                "macro table must be strictly ordered by MacroId",
            );
        }
        let mut macro_ids = BTreeSet::new();
        for (index, macro_item) in self.package.macros().iter().enumerate() {
            let path = format!("macros[{index}]");
            if !macro_ids.insert(macro_item.id) {
                self.push(
                    ContractViolationCode::DuplicateId,
                    &format!("{path}.id"),
                    "duplicate MacroId",
                );
            }
            match MacroId::named(macro_item.identity_file, &macro_item.name) {
                Ok(expected) if expected == macro_item.id => {}
                _ => self.push(
                    ContractViolationCode::InvalidId,
                    &format!("{path}.id"),
                    "MacroId does not match identity file and name",
                ),
            }
            if !self.files.contains_key(&macro_item.identity_file) {
                self.push(
                    ContractViolationCode::InvalidReference,
                    &format!("{path}.identity_file"),
                    "macro identity file does not exist",
                );
            }
            if macro_item.occurrences.is_empty() {
                self.push(
                    ContractViolationCode::InvalidId,
                    &format!("{path}.occurrences"),
                    "macro must contain at least one occurrence",
                );
            }
            if !strictly_sorted_by_key(&macro_item.occurrences, |occurrence| occurrence.id) {
                self.push(
                    ContractViolationCode::NonCanonicalOrder,
                    &format!("{path}.occurrences"),
                    "macro occurrences must be strictly ordered by OccurrenceId",
                );
            }
            for (occurrence_index, occurrence) in macro_item.occurrences.iter().enumerate() {
                let occurrence_path = format!("{path}.occurrences[{occurrence_index}]");
                self.validate_range(&occurrence.range, &format!("{occurrence_path}.range"));
                if occurrence.range.start == occurrence.range.end {
                    self.push(
                        ContractViolationCode::InvalidRange,
                        &format!("{occurrence_path}.range"),
                        "macro occurrence range must be nonempty",
                    );
                }
                self.validate_provenance(
                    &occurrence.provenance,
                    occurrence.range.file,
                    &occurrence_path,
                );
                if !self.occurrence_ids.insert(occurrence.id) {
                    self.push(
                        ContractViolationCode::DuplicateId,
                        &format!("{occurrence_path}.id"),
                        "duplicate OccurrenceId",
                    );
                }
                let expected = OccurrenceId::derive_macro(
                    macro_item.id,
                    occurrence.range.file,
                    &canonical_tokens_bytes(&occurrence.normalized_tokens),
                    occurrence.duplicate_ordinal,
                );
                if expected != occurrence.id {
                    self.push(
                        ContractViolationCode::InvalidId,
                        &format!("{occurrence_path}.id"),
                        "macro OccurrenceId does not match its identity inputs",
                    );
                }
            }
        }
    }

    fn validate_diagnostics(&mut self) {
        if self
            .package
            .diagnostics()
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "diagnostics",
                "diagnostics must be strictly ordered by their canonical domain fields",
            );
        }
        for (index, diagnostic) in self.package.diagnostics().iter().enumerate() {
            let path = format!("diagnostics[{index}]");
            if diagnostic.message.is_empty() {
                self.push(
                    ContractViolationCode::InvalidName,
                    &format!("{path}.message"),
                    "diagnostic message must be nonempty",
                );
            }
            if diagnostic.target != self.package.target_fingerprint() {
                self.push(
                    ContractViolationCode::InvalidTarget,
                    &format!("{path}.target"),
                    "diagnostic target differs from package target",
                );
            }
            if let Some(range) = diagnostic.range {
                self.validate_range(&range, &format!("{path}.range"));
            }
            for (related_index, related) in diagnostic.related.iter().enumerate() {
                self.validate_range(
                    &related.range,
                    &format!("{path}.related[{related_index}].range"),
                );
            }
            if diagnostic.related.windows(2).any(|pair| pair[0] >= pair[1]) {
                self.push(
                    ContractViolationCode::NonCanonicalOrder,
                    &format!("{path}.related"),
                    "related source locations must be strictly ordered and unique",
                );
            }
            if diagnostic
                .declaration
                .is_some_and(|id| !self.declarations.contains_key(&id))
            {
                self.push(
                    ContractViolationCode::InvalidReference,
                    &format!("{path}.declaration"),
                    "diagnostic references unknown declaration",
                );
            }
        }
    }

    fn validate_inputs(&mut self) {
        let inputs = self.package.inputs().clone();
        if inputs.entry_files.is_empty() {
            self.push(
                ContractViolationCode::InvalidFile,
                "inputs.entry_files",
                "at least one entry file is required",
            );
        }
        if !strictly_sorted_by_key(&inputs.entry_files, |file| *file) {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "inputs.entry_files",
                "entry files must be a strictly FileId-ordered set",
            );
        }
        for (index, file) in self.package.inputs().entry_files.iter().enumerate() {
            match self.files.get(file) {
                None => self.push(
                    ContractViolationCode::InvalidReference,
                    &format!("inputs.entry_files[{index}]"),
                    "entry file does not exist",
                ),
                Some(source) if source.role != SourceFileRole::Entry => self.push(
                    ContractViolationCode::InvalidFile,
                    &format!("inputs.entry_files[{index}]"),
                    "entry file list references a non-entry SourceFile",
                ),
                Some(_) => {}
            }
        }
        for (index, source) in self.package.files().iter().enumerate() {
            if source.role == SourceFileRole::Entry
                && inputs.entry_files.binary_search(&source.id).is_err()
            {
                self.push(
                    ContractViolationCode::InvalidFile,
                    format!("files[{index}].role"),
                    "entry SourceFile is missing from inputs.entry_files",
                );
            }
        }
        for (index, file) in inputs.forced_includes.iter().enumerate() {
            if !self.files.contains_key(file) {
                self.push(
                    ContractViolationCode::InvalidReference,
                    &format!("inputs.forced_includes[{index}]"),
                    "forced include does not exist",
                );
            }
        }
        for (index, include) in inputs.include_search.iter().enumerate() {
            if normalize_logical_path(&include.logical_path).as_deref()
                != Ok(include.logical_path.as_str())
                || !safe_text(&include.logical_path)
            {
                self.push(
                    ContractViolationCode::InvalidFile,
                    &format!("inputs.include_search[{index}].logical_path"),
                    "include path is not canonical",
                );
            }
        }
        for (index, event) in inputs.define_events.iter().enumerate() {
            let (name, value) = match event {
                DefineEvent::Define { name, value } => (name, value.as_deref()),
                DefineEvent::Undefine { name } => (name, None),
            };
            if !canonical_c_identifier(name) {
                self.push(
                    ContractViolationCode::InvalidName,
                    format!("inputs.define_events[{index}].name"),
                    "define/undefine name must be a canonical C identifier",
                );
            }
            if value.is_some_and(|value| !safe_text(value)) {
                self.push(
                    ContractViolationCode::InvalidName,
                    format!("inputs.define_events[{index}].value"),
                    "define value contains a NUL or control character",
                );
            }
        }
        match &inputs.preprocessor {
            PreprocessorIdentity::Builtin {
                implementation_version,
            } => {
                if implementation_version.is_empty() || !safe_text(implementation_version) {
                    self.push(
                        ContractViolationCode::InvalidName,
                        "inputs.preprocessor.implementation_version",
                        "builtin preprocessor version must be nonempty safe text",
                    );
                }
            }
            PreprocessorIdentity::External {
                executable,
                arguments,
                ..
            } => {
                if normalize_logical_path(executable).as_deref() != Ok(executable.as_str())
                    || !safe_text(executable)
                {
                    self.push(
                        ContractViolationCode::InvalidFile,
                        "inputs.preprocessor.executable",
                        "external preprocessor executable must be a canonical logical path",
                    );
                }
                for (index, argument) in arguments.iter().enumerate() {
                    if !safe_text(argument) {
                        self.push(
                            ContractViolationCode::InvalidName,
                            format!("inputs.preprocessor.arguments[{index}]"),
                            "external preprocessor argument contains a NUL or control character",
                        );
                    }
                }
            }
        }
        if let EnvironmentInputs::Captured { variables } = &inputs.environment {
            if variables
                .windows(2)
                .any(|pair| pair[0].name >= pair[1].name)
            {
                self.push(
                    ContractViolationCode::NonCanonicalOrder,
                    "inputs.environment.variables",
                    "captured environment variables must be strictly name-ordered and unique",
                );
            }
            for (index, variable) in variables.iter().enumerate() {
                if !canonical_environment_name(variable) {
                    self.push(
                        ContractViolationCode::InvalidName,
                        format!("inputs.environment.variables[{index}].name"),
                        "captured environment name must be nonempty canonical safe text without '='",
                    );
                }
            }
        }
    }

    fn validate_alias_cycles(&mut self) {
        let mut aliases = HashMap::new();
        for declaration in self.package.declarations() {
            if let SourceDeclarationKind::TypeAlias(alias) = &declaration.kind {
                let mut direct = Vec::new();
                collect_alias_refs(&alias.target, &mut direct);
                aliases.insert(declaration.id, direct);
            }
        }
        let mut complete = HashSet::new();
        let mut active = HashSet::new();
        for id in aliases.keys().copied() {
            if alias_cycle(id, &aliases, &mut active, &mut complete) {
                self.push(
                    ContractViolationCode::AliasCycle,
                    "declarations",
                    format!("alias cycle includes {id}"),
                );
            }
        }
    }

    fn validate_completeness(&mut self) {
        let completeness = self.package.completeness().clone();
        match &completeness {
            Completeness::Complete => {
                if self.package.diagnostics().iter().any(|diagnostic| {
                    diagnostic.stage == DiagnosticStage::Recovery
                        || diagnostic.completeness_impact
                            != DiagnosticCompletenessImpact::Informational
                        || (diagnostic.severity == Severity::Error
                            && matches!(
                                diagnostic.stage,
                                DiagnosticStage::Configuration
                                    | DiagnosticStage::Preprocess
                                    | DiagnosticStage::Parse
                            ))
                }) {
                    self.push(ContractViolationCode::InvalidCompleteness, "completeness", "complete package contains recovery, a source-stage error, or a diagnostic that forces incompleteness");
                }
            }
            Completeness::Partial { reasons } => {
                self.validate_reason_order(reasons);
                if !self.package.diagnostics().iter().any(|diagnostic| {
                    diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesPartial
                }) {
                    self.push(
                        ContractViolationCode::InvalidCompleteness,
                        "completeness",
                        "partial completeness requires at least one ForcesPartial diagnostic",
                    );
                }
                if self.package.diagnostics().iter().any(|diagnostic| {
                    diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesRejected
                }) {
                    self.push(
                        ContractViolationCode::InvalidCompleteness,
                        "completeness",
                        "a ForcesRejected diagnostic requires rejected completeness",
                    );
                }
                self.validate_completeness_correspondence(reasons);
            }
            Completeness::Rejected { reasons } => {
                self.validate_reason_order(reasons);
                if !self.package.diagnostics().iter().any(|diagnostic| {
                    diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesRejected
                }) {
                    self.push(
                        ContractViolationCode::InvalidCompleteness,
                        "completeness",
                        "rejected completeness requires at least one ForcesRejected diagnostic",
                    );
                }
                self.validate_completeness_correspondence(reasons);
            }
        }
    }

    fn validate_reason_order(&mut self, reasons: &[super::CompletenessReason]) {
        if reasons.windows(2).any(|pair| pair[0] >= pair[1]) {
            self.push(
                ContractViolationCode::NonCanonicalOrder,
                "completeness.reasons",
                "completeness reasons must be strictly ordered and unique",
            );
        }
    }

    fn validate_completeness_correspondence(&mut self, reasons: &[super::CompletenessReason]) {
        let diagnostics = self.package.diagnostics();
        for (index, reason) in reasons.iter().enumerate() {
            if !diagnostics.iter().any(|diagnostic| {
                diagnostic.completeness_impact != DiagnosticCompletenessImpact::Informational
                    && diagnostic.code == reason.code
                    && diagnostic.message == reason.message
                    && diagnostic.range == reason.range
            }) {
                self.push(
                    ContractViolationCode::InvalidCompleteness,
                    format!("completeness.reasons[{index}]"),
                    "completeness reason has no exactly matching forcing diagnostic",
                );
            }
        }
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            if diagnostic.completeness_impact != DiagnosticCompletenessImpact::Informational
                && !reasons.iter().any(|reason| {
                    diagnostic.code == reason.code
                        && diagnostic.message == reason.message
                        && diagnostic.range == reason.range
                })
            {
                self.push(
                    ContractViolationCode::InvalidCompleteness,
                    format!("diagnostics[{index}].completeness_impact"),
                    "forcing diagnostic has no exactly matching completeness reason",
                );
            }
        }
    }

    fn validate_attributes(&mut self, attributes: &[SourceAttribute], path: &str) {
        for (index, attribute) in attributes.iter().enumerate() {
            if attribute.name.is_empty() || attribute.spelling.is_empty() {
                self.push(
                    ContractViolationCode::InvalidName,
                    &format!("{path}.attributes[{index}]"),
                    "attribute name and spelling must be nonempty",
                );
            }
            self.validate_range(
                &attribute.range,
                &format!("{path}.attributes[{index}].range"),
            );
        }
    }

    fn validate_provenance(
        &mut self,
        provenance: &SourceProvenance,
        anchor_file: FileId,
        path: &str,
    ) {
        let expected_role = match provenance.origin {
            SourceOrigin::Entry => SourceFileRole::Entry,
            SourceOrigin::UserInclude => SourceFileRole::UserInclude,
            SourceOrigin::SystemInclude => SourceFileRole::SystemInclude,
            SourceOrigin::Builtin => SourceFileRole::Builtin,
            SourceOrigin::Generated => SourceFileRole::Generated,
        };
        if self
            .files
            .get(&anchor_file)
            .is_some_and(|file| file.role != expected_role)
        {
            self.push(
                ContractViolationCode::InvalidFile,
                format!("{path}.provenance.origin"),
                "provenance origin does not match the role of the ranged source file",
            );
        }
        for (index, include) in provenance.include_chain.iter().enumerate() {
            self.validate_range(
                &include.directive,
                &format!("{path}.provenance.include_chain[{index}].directive"),
            );
            if !self.files.contains_key(&include.included) {
                self.push(
                    ContractViolationCode::InvalidReference,
                    &format!("{path}.provenance.include_chain[{index}].included"),
                    "include provenance references unknown file",
                );
            }
        }
        for (index, expansion) in provenance.macro_expansions.iter().enumerate() {
            self.validate_range(
                &expansion.invocation,
                &format!("{path}.provenance.macro_expansions[{index}].invocation"),
            );
            if let Some(definition) = expansion.definition {
                self.validate_range(
                    &definition,
                    &format!("{path}.provenance.macro_expansions[{index}].definition"),
                );
            }
        }
    }

    fn validate_range(&mut self, range: &SourceRange, path: &str) {
        let Some(file) = self.files.get(&range.file) else {
            self.push(
                ContractViolationCode::InvalidReference,
                path,
                "range references unknown file",
            );
            return;
        };
        if range.start > range.end || range.end > file.byte_len {
            self.push(
                ContractViolationCode::InvalidRange,
                path,
                "range is reversed or exceeds file length",
            );
        }
    }

    fn push(
        &mut self,
        code: ContractViolationCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.violations.push(ContractViolation {
            code,
            path: path.into(),
            message: message.into(),
        });
    }
}

#[derive(Clone, Copy)]
enum ReferenceKind {
    Alias,
    Record,
    Enum,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeContext {
    General,
    Parameter,
    RecordField { flexible_allowed: bool },
}

impl TypeContext {
    const fn flexible_allowed(self) -> bool {
        matches!(
            self,
            Self::RecordField {
                flexible_allowed: true
            }
        )
    }

    const fn is_record_field(self) -> bool {
        matches!(self, Self::RecordField { .. })
    }
}

fn safe_text(value: &str) -> bool {
    !value.chars().any(char::is_control)
}

fn source_range_contains(outer: SourceRange, inner: SourceRange) -> bool {
    outer.file == inner.file && outer.start <= inner.start && inner.end <= outer.end
}

fn canonical_c_identifier(value: &str) -> bool {
    if normalize_identifier(value).as_deref() != Ok(value) || !safe_text(value) {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && chars.all(|character| character == '_' || character.is_alphanumeric())
}

fn canonical_environment_name(variable: &CapturedEnvironment) -> bool {
    let name = &variable.name;
    !name.is_empty()
        && !name.contains('=')
        && safe_text(name)
        && normalize_identifier(name).as_deref() == Ok(name.as_str())
}

fn strictly_sorted_by_key<T, K: Ord>(values: &[T], mut key: impl FnMut(&T) -> K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn collect_alias_refs(ty: &CType, output: &mut Vec<DeclarationId>) {
    match &ty.kind {
        CTypeKind::AliasRef(id) => output.push(*id),
        CTypeKind::Pointer(inner) => collect_alias_refs(inner, output),
        CTypeKind::Array { element, .. } => collect_alias_refs(element, output),
        CTypeKind::Function(function) => {
            collect_alias_refs(&function.return_type, output);
            for parameter in &function.parameters {
                collect_alias_refs(&parameter.ty, output);
            }
        }
        CTypeKind::Void
        | CTypeKind::Bool
        | CTypeKind::Integer(_)
        | CTypeKind::Floating(_)
        | CTypeKind::Complex(_)
        | CTypeKind::RecordRef(_)
        | CTypeKind::EnumRef(_)
        | CTypeKind::Unsupported { .. } => {}
    }
}

fn alias_cycle(
    id: DeclarationId,
    graph: &HashMap<DeclarationId, Vec<DeclarationId>>,
    active: &mut HashSet<DeclarationId>,
    complete: &mut HashSet<DeclarationId>,
) -> bool {
    if complete.contains(&id) {
        return false;
    }
    if !active.insert(id) {
        return true;
    }
    let cyclic = graph
        .get(&id)
        .into_iter()
        .flatten()
        .copied()
        .filter(|next| graph.contains_key(next))
        .any(|next| alias_cycle(next, graph, active, complete));
    active.remove(&id);
    complete.insert(id);
    cyclic
}
