//! Checked projection and composition of immutable source packages.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use super::{
    CType, CTypeKind, Completeness, CompletenessReason, DeclarationId, DeclarationIdentity,
    DiagnosticCompletenessImpact, EffectiveSourceInputs, EntityScope, FileId, MacroId, Selection,
    SourceDeclaration, SourceDeclarationKind, SourceDiagnostic, SourceFile, SourceMacro,
    SourcePackage, SourcePackageBuildError, SourcePackageInput,
};

/// A checked retain or merge operation could not preserve schema-v2 meaning.
#[derive(Debug, Error)]
pub enum ComposeError {
    #[error("selected or transitive declaration {id} is missing")]
    MissingDeclaration { id: DeclarationId },
    #[error("opaque selection root {id} is not a record declaration")]
    OpaqueRootIsNotRecord { id: DeclarationId },
    #[error("source packages have incompatible targets")]
    IncompatibleTarget,
    #[error("source packages have incompatible effective input field {field}")]
    IncompatibleSourceInputs { field: &'static str },
    #[error("source file {id} has conflicting definitions")]
    ConflictingFile { id: FileId },
    #[error("declaration {id} has conflicting definitions")]
    ConflictingDeclaration { id: DeclarationId },
    #[error("macro {id} has conflicting definitions")]
    ConflictingMacro { id: MacroId },
    #[error("composed source package is invalid: {0}")]
    InvalidPackage(#[from] SourcePackageBuildError),
}

impl SourcePackage {
    /// Returns a checked package containing the selected declaration roots and
    /// every declaration referenced by their identities or types.
    ///
    /// Files, effective inputs, and macros are retained because they are part
    /// of the scan identity. Declaration-bound diagnostics for removed
    /// declarations are discarded; completeness is then derived exactly from
    /// the remaining forcing diagnostics.
    pub fn retain(&self, selection: &Selection) -> Result<Self, ComposeError> {
        let roots = match selection {
            Selection::AllSupported => self
                .declarations()
                .iter()
                .filter(|declaration| declaration.support.is_supported())
                .map(|declaration| declaration.id)
                .collect::<Vec<_>>(),
            Selection::Only(ids) => ids.clone(),
            Selection::OpaqueOnly(ids) => {
                for id in ids {
                    let declaration = self
                        .declaration(*id)
                        .ok_or(ComposeError::MissingDeclaration { id: *id })?;
                    if !matches!(declaration.kind, SourceDeclarationKind::Record(_)) {
                        return Err(ComposeError::OpaqueRootIsNotRecord { id: *id });
                    }
                }
                ids.clone()
            }
        };

        let retained = declaration_closure(self, roots)?;
        let declarations = self
            .declarations()
            .iter()
            .filter(|declaration| retained.contains(&declaration.id))
            .cloned()
            .collect();
        let diagnostics = self
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .declaration
                    .is_none_or(|id| retained.contains(&id))
            })
            .cloned()
            .collect::<Vec<_>>();

        Ok(Self::try_new(SourcePackageInput {
            target: self.target().clone(),
            files: self.files().to_vec(),
            inputs: self.inputs().clone(),
            declarations,
            macros: self.macros().to_vec(),
            completeness: completeness_from_diagnostics(&diagnostics),
            diagnostics,
        })?)
    }

    /// Merges two independently checked source packages when their target and
    /// effective preprocessing inputs are compatible.
    ///
    /// Translation-unit entry sets and disjoint file/declaration/macro tables
    /// are unioned. Equal stable IDs must describe equal attached data; only
    /// occurrence sets may be unioned for equal declarations and macros.
    pub fn merge(self, other: Self) -> Result<Self, ComposeError> {
        if self.target() != other.target() {
            return Err(ComposeError::IncompatibleTarget);
        }
        let inputs = merge_inputs(self.inputs(), other.inputs())?;

        let files = merge_files(self.files(), other.files())?;
        let declarations = merge_declarations(self.declarations(), other.declarations())?;
        let macros = merge_macros(self.macros(), other.macros())?;
        let diagnostics = self
            .diagnostics()
            .iter()
            .chain(other.diagnostics())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        Ok(Self::try_new(SourcePackageInput {
            target: self.target().clone(),
            files,
            inputs,
            declarations,
            macros,
            completeness: completeness_from_diagnostics(&diagnostics),
            diagnostics,
        })?)
    }
}

fn declaration_closure(
    package: &SourcePackage,
    roots: Vec<DeclarationId>,
) -> Result<BTreeSet<DeclarationId>, ComposeError> {
    let mut retained = BTreeSet::new();
    let mut queue = VecDeque::from(roots);
    while let Some(id) = queue.pop_front() {
        if !retained.insert(id) {
            continue;
        }
        let declaration = package
            .declaration(id)
            .ok_or(ComposeError::MissingDeclaration { id })?;
        if let DeclarationIdentity::Named {
            scope: EntityScope::Owner(owner),
            ..
        }
        | DeclarationIdentity::Anonymous {
            scope: EntityScope::Owner(owner),
            ..
        } = &declaration.identity
        {
            queue.push_back(*owner);
        }
        enqueue_declaration_references(declaration, &mut queue);
    }
    Ok(retained)
}

fn enqueue_declaration_references(
    declaration: &SourceDeclaration,
    queue: &mut VecDeque<DeclarationId>,
) {
    match &declaration.kind {
        SourceDeclarationKind::Function(function) => {
            enqueue_type_references(&function.return_type, queue);
            for parameter in &function.parameters {
                enqueue_type_references(&parameter.ty, queue);
            }
        }
        SourceDeclarationKind::Record(record) => {
            for field in &record.fields {
                enqueue_type_references(&field.ty, queue);
            }
        }
        SourceDeclarationKind::Enum(enumeration) => {
            if let Some(underlying) = &enumeration.explicit_underlying_type {
                enqueue_type_references(underlying, queue);
            }
        }
        SourceDeclarationKind::TypeAlias(alias) => enqueue_type_references(&alias.target, queue),
        SourceDeclarationKind::Variable(variable) => enqueue_type_references(&variable.ty, queue),
        SourceDeclarationKind::Unsupported(_) => {}
    }
}

fn enqueue_type_references(ty: &CType, queue: &mut VecDeque<DeclarationId>) {
    match &ty.kind {
        CTypeKind::Pointer(inner) | CTypeKind::Array { element: inner, .. } => {
            enqueue_type_references(inner, queue);
        }
        CTypeKind::Function(function) => {
            enqueue_type_references(&function.return_type, queue);
            for parameter in &function.parameters {
                enqueue_type_references(&parameter.ty, queue);
            }
        }
        CTypeKind::AliasRef(id) | CTypeKind::RecordRef(id) | CTypeKind::EnumRef(id) => {
            queue.push_back(*id);
        }
        CTypeKind::Void
        | CTypeKind::Bool
        | CTypeKind::Integer(_)
        | CTypeKind::Floating(_)
        | CTypeKind::Complex(_)
        | CTypeKind::Unsupported { .. } => {}
    }
}

fn merge_inputs(
    left: &EffectiveSourceInputs,
    right: &EffectiveSourceInputs,
) -> Result<EffectiveSourceInputs, ComposeError> {
    macro_rules! require_equal {
        ($field:ident) => {
            if left.$field != right.$field {
                return Err(ComposeError::IncompatibleSourceInputs {
                    field: stringify!($field),
                });
            }
        };
    }
    require_equal!(include_search);
    require_equal!(define_events);
    require_equal!(forced_includes);
    require_equal!(preprocessor);
    require_equal!(environment);
    require_equal!(path_mapping_fingerprint);

    let entry_files = left
        .entry_files
        .iter()
        .chain(&right.entry_files)
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(EffectiveSourceInputs {
        entry_files,
        include_search: left.include_search.clone(),
        define_events: left.define_events.clone(),
        forced_includes: left.forced_includes.clone(),
        preprocessor: left.preprocessor.clone(),
        environment: left.environment.clone(),
        path_mapping_fingerprint: left.path_mapping_fingerprint,
    })
}

fn merge_files(left: &[SourceFile], right: &[SourceFile]) -> Result<Vec<SourceFile>, ComposeError> {
    let mut files = BTreeMap::<FileId, SourceFile>::new();
    for file in left.iter().chain(right).cloned() {
        match files.entry(file.id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(file);
            }
            std::collections::btree_map::Entry::Occupied(entry) if entry.get() == &file => {}
            std::collections::btree_map::Entry::Occupied(_) => {
                return Err(ComposeError::ConflictingFile { id: file.id });
            }
        }
    }
    Ok(files.into_values().collect())
}

fn merge_declarations(
    left: &[SourceDeclaration],
    right: &[SourceDeclaration],
) -> Result<Vec<SourceDeclaration>, ComposeError> {
    let mut declarations = BTreeMap::<DeclarationId, SourceDeclaration>::new();
    for declaration in left.iter().chain(right).cloned() {
        match declarations.entry(declaration.id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(declaration);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if current.identity != declaration.identity
                    || current.name != declaration.name
                    || current.linkage != declaration.linkage
                    || current.visibility != declaration.visibility
                    || current.support != declaration.support
                    || current.kind != declaration.kind
                {
                    return Err(ComposeError::ConflictingDeclaration { id: declaration.id });
                }
                current.occurrences = merge_occurrences(
                    declaration.id,
                    &current.occurrences,
                    &declaration.occurrences,
                )?;
            }
        }
    }
    Ok(declarations.into_values().collect())
}

fn merge_occurrences(
    declaration: DeclarationId,
    left: &[super::DeclarationOccurrence],
    right: &[super::DeclarationOccurrence],
) -> Result<Vec<super::DeclarationOccurrence>, ComposeError> {
    let mut occurrences = BTreeMap::new();
    for occurrence in left.iter().chain(right).cloned() {
        match occurrences.entry(occurrence.id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(occurrence);
            }
            std::collections::btree_map::Entry::Occupied(entry) if entry.get() == &occurrence => {}
            std::collections::btree_map::Entry::Occupied(_) => {
                return Err(ComposeError::ConflictingDeclaration { id: declaration });
            }
        }
    }
    Ok(occurrences.into_values().collect())
}

fn merge_macros(
    left: &[SourceMacro],
    right: &[SourceMacro],
) -> Result<Vec<SourceMacro>, ComposeError> {
    let mut macros = BTreeMap::<MacroId, SourceMacro>::new();
    for macro_item in left.iter().chain(right).cloned() {
        match macros.entry(macro_item.id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(macro_item);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                if current.identity_file != macro_item.identity_file
                    || current.name != macro_item.name
                    || current.form != macro_item.form
                    || current.category != macro_item.category
                    || current.body != macro_item.body
                    || current.normalized_tokens != macro_item.normalized_tokens
                    || current.value != macro_item.value
                    || current.support != macro_item.support
                {
                    return Err(ComposeError::ConflictingMacro { id: macro_item.id });
                }
                let mut occurrences = BTreeMap::new();
                for occurrence in current
                    .occurrences
                    .iter()
                    .chain(&macro_item.occurrences)
                    .cloned()
                {
                    match occurrences.entry(occurrence.id) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(occurrence);
                        }
                        std::collections::btree_map::Entry::Occupied(entry)
                            if entry.get() == &occurrence => {}
                        std::collections::btree_map::Entry::Occupied(_) => {
                            return Err(ComposeError::ConflictingMacro { id: macro_item.id });
                        }
                    }
                }
                current.occurrences = occurrences.into_values().collect();
            }
        }
    }
    Ok(macros.into_values().collect())
}

fn completeness_from_diagnostics(diagnostics: &[SourceDiagnostic]) -> Completeness {
    let reasons = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.completeness_impact != DiagnosticCompletenessImpact::Informational
        })
        .map(|diagnostic| CompletenessReason {
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
            range: diagnostic.range,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if diagnostics.iter().any(|diagnostic| {
        diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesRejected
    }) {
        Completeness::Rejected { reasons }
    } else if !reasons.is_empty() {
        Completeness::Partial { reasons }
    } else {
        Completeness::Complete
    }
}
