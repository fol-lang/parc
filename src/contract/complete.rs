//! Proof wrapper for a complete, supported declaration closure.

use std::collections::{BTreeMap, VecDeque};

use thiserror::Error;

use super::{
    ArrayBound, AttributeDisposition, CType, CTypeKind, CallingConvention, Completeness,
    CompletenessReason, DeclarationId, Selection, SourceDeclaration, SourceDeclarationKind,
    SourcePackage, SupportStatus,
};

/// A package plus the exact selected transitive declaration closure for which
/// source completeness and support were proved.
///
/// Fields are private and this type has no serde implementation. It can only
/// be obtained through [`SourcePackage::into_complete`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteSourcePackage {
    package: SourcePackage,
    selection: Selection,
    declaration_closure: Vec<DeclarationClosureEntry>,
}

/// The level of source definition proved necessary for one selected closure
/// member. Downstream ABI stages use this to avoid demanding measured layout
/// for records reached only through pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClosureRequirement {
    Opaque,
    Definition,
}

/// One immutable, DeclarationId-ordered member of a complete source closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclarationClosureEntry {
    declaration: DeclarationId,
    requirement: ClosureRequirement,
}

impl DeclarationClosureEntry {
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    pub const fn requirement(&self) -> ClosureRequirement {
        self.requirement
    }
}

impl CompleteSourcePackage {
    pub fn package(&self) -> &SourcePackage {
        &self.package
    }

    /// Semantic alias used by downstream typed stages.
    pub fn source(&self) -> &SourcePackage {
        &self.package
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn declaration_closure(&self) -> &[DeclarationClosureEntry] {
        &self.declaration_closure
    }

    pub fn into_package(self) -> SourcePackage {
        self.package
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompletionBlocker {
    #[error("source package is partial or rejected")]
    PackageIncomplete { reasons: Vec<CompletenessReason> },
    #[error("selected or transitive declaration {id} is missing")]
    MissingDeclaration { id: DeclarationId },
    #[error("declaration {id} is not fully supported at {path}: {reason}")]
    Unsupported {
        id: DeclarationId,
        path: String,
        reason: String,
    },
    #[error("record declaration {id} is incomplete but a complete definition is required")]
    IncompleteRecord { id: DeclarationId },
    #[error("opaque selection root {id} is not a record declaration")]
    OpaqueRootIsNotRecord { id: DeclarationId },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("selected source closure has {count} blocker(s)", count = .blockers.len())]
pub struct IncompleteSource {
    blockers: Vec<CompletionBlocker>,
}

impl IncompleteSource {
    pub fn blockers(&self) -> &[CompletionBlocker] {
        &self.blockers
    }

    pub fn into_blockers(self) -> Vec<CompletionBlocker> {
        self.blockers
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Need {
    OpaqueSufficient,
    Definition,
}

impl SourcePackage {
    /// Consumes a checked package and proves completeness for the selected
    /// roots and every declaration reached from their types.
    pub fn into_complete(
        self,
        selected: &Selection,
    ) -> Result<CompleteSourcePackage, IncompleteSource> {
        let mut blockers = Vec::new();
        match self.completeness() {
            Completeness::Complete => {}
            Completeness::Partial { reasons } | Completeness::Rejected { reasons } => {
                blockers.push(CompletionBlocker::PackageIncomplete {
                    reasons: reasons.clone(),
                });
            }
        }

        let roots: Vec<_> = match selected {
            Selection::AllSupported => self
                .declarations()
                .iter()
                .filter(|declaration| declaration.support.is_supported())
                .map(|declaration| {
                    let need = match &declaration.kind {
                        SourceDeclarationKind::Record(record)
                            if record.completeness == super::RecordCompleteness::Incomplete =>
                        {
                            Need::OpaqueSufficient
                        }
                        _ => Need::Definition,
                    };
                    (declaration.id, need)
                })
                .collect(),
            Selection::Only(ids) => ids
                .iter()
                .copied()
                .map(|id| (id, Need::Definition))
                .collect(),
            Selection::OpaqueOnly(ids) => ids
                .iter()
                .copied()
                .map(|id| (id, Need::OpaqueSufficient))
                .collect(),
        };

        let mut required = BTreeMap::<DeclarationId, Need>::new();
        let mut queue = VecDeque::new();
        for (id, need) in roots {
            enqueue(&mut required, &mut queue, id, need);
        }

        while let Some((id, need)) = queue.pop_front() {
            // An earlier opaque visit may have been upgraded while queued.
            let current_need = required.get(&id).copied().unwrap_or(need);
            if current_need != need && need == Need::OpaqueSufficient {
                continue;
            }
            let Some(declaration) = self.declaration(id) else {
                blockers.push(CompletionBlocker::MissingDeclaration { id });
                continue;
            };
            if matches!(selected, Selection::OpaqueOnly(_))
                && need == Need::OpaqueSufficient
                && !matches!(declaration.kind, SourceDeclarationKind::Record(_))
            {
                blockers.push(CompletionBlocker::OpaqueRootIsNotRecord { id });
                continue;
            }
            validate_declaration_closure(
                declaration,
                current_need,
                &mut required,
                &mut queue,
                &mut blockers,
            );
        }

        blockers.sort_by_key(|left| left.to_string());
        blockers.dedup();
        if !blockers.is_empty() {
            return Err(IncompleteSource { blockers });
        }

        Ok(CompleteSourcePackage {
            package: self,
            selection: selected.clone(),
            declaration_closure: required
                .into_iter()
                .map(|(declaration, need)| DeclarationClosureEntry {
                    declaration,
                    requirement: match need {
                        Need::OpaqueSufficient => ClosureRequirement::Opaque,
                        Need::Definition => ClosureRequirement::Definition,
                    },
                })
                .collect(),
        })
    }
}

fn validate_declaration_closure(
    declaration: &SourceDeclaration,
    need: Need,
    required: &mut BTreeMap<DeclarationId, Need>,
    queue: &mut VecDeque<(DeclarationId, Need)>,
    blockers: &mut Vec<CompletionBlocker>,
) {
    let id = declaration.id;
    reject_status(id, "support", &declaration.support, blockers);
    for (index, occurrence) in declaration.occurrences.iter().enumerate() {
        reject_attributes(
            id,
            &format!("occurrences[{index}].attributes"),
            &occurrence.attributes,
            blockers,
        );
    }

    match &declaration.kind {
        SourceDeclarationKind::Function(function) => {
            if let CallingConvention::Unsupported { spelling } = &function.calling_convention {
                reject(id, "kind.function.calling_convention", spelling, blockers);
            }
            visit_type(
                id,
                "kind.function.return_type",
                &function.return_type,
                false,
                required,
                queue,
                blockers,
            );
            for (index, parameter) in function.parameters.iter().enumerate() {
                let path = format!("kind.function.parameters[{index}]");
                reject_status(id, &format!("{path}.support"), &parameter.support, blockers);
                reject_attributes(
                    id,
                    &format!("{path}.attributes"),
                    &parameter.attributes,
                    blockers,
                );
                visit_type(
                    id,
                    &format!("{path}.ty"),
                    &parameter.ty,
                    false,
                    required,
                    queue,
                    blockers,
                );
            }
        }
        SourceDeclarationKind::Record(record) => {
            if need == Need::Definition
                && record.completeness == super::RecordCompleteness::Incomplete
            {
                blockers.push(CompletionBlocker::IncompleteRecord { id });
            }
            if need == Need::Definition {
                for (index, field) in record.fields.iter().enumerate() {
                    let path = format!("kind.record.fields[{index}]");
                    reject_status(id, &format!("{path}.support"), &field.support, blockers);
                    reject_attributes(
                        id,
                        &format!("{path}.attributes"),
                        &field.attributes,
                        blockers,
                    );
                    visit_type(
                        id,
                        &format!("{path}.ty"),
                        &field.ty,
                        false,
                        required,
                        queue,
                        blockers,
                    );
                }
            }
        }
        SourceDeclarationKind::Enum(enumeration) => {
            if let Some(underlying) = &enumeration.explicit_underlying_type {
                visit_type(
                    id,
                    "kind.enum.explicit_underlying_type",
                    underlying,
                    false,
                    required,
                    queue,
                    blockers,
                );
            }
            for (index, variant) in enumeration.variants.iter().enumerate() {
                let path = format!("kind.enum.variants[{index}]");
                reject_status(id, &format!("{path}.support"), &variant.support, blockers);
                reject_attributes(
                    id,
                    &format!("{path}.attributes"),
                    &variant.attributes,
                    blockers,
                );
            }
        }
        SourceDeclarationKind::TypeAlias(alias) => visit_type(
            id,
            "kind.type_alias.target",
            &alias.target,
            need == Need::OpaqueSufficient,
            required,
            queue,
            blockers,
        ),
        SourceDeclarationKind::Variable(variable) => visit_type(
            id,
            "kind.variable.ty",
            &variable.ty,
            false,
            required,
            queue,
            blockers,
        ),
        SourceDeclarationKind::Unsupported(unsupported) => {
            reject(id, "kind.unsupported", &unsupported.spelling, blockers)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn visit_type(
    owner: DeclarationId,
    path: &str,
    ty: &CType,
    behind_pointer: bool,
    required: &mut BTreeMap<DeclarationId, Need>,
    queue: &mut VecDeque<(DeclarationId, Need)>,
    blockers: &mut Vec<CompletionBlocker>,
) {
    reject_status(owner, &format!("{path}.support"), &ty.support, blockers);
    match &ty.kind {
        CTypeKind::Pointer(pointee) => visit_type(
            owner,
            &format!("{path}.pointee"),
            pointee,
            true,
            required,
            queue,
            blockers,
        ),
        CTypeKind::Array { element, bound, .. } => {
            if let ArrayBound::Invalid { spelling, .. } = bound {
                reject(owner, format!("{path}.bound"), spelling, blockers);
            }
            visit_type(
                owner,
                &format!("{path}.element"),
                element,
                behind_pointer,
                required,
                queue,
                blockers,
            );
        }
        CTypeKind::Function(function) => {
            if let CallingConvention::Unsupported { spelling } = &function.calling_convention {
                reject(
                    owner,
                    format!("{path}.calling_convention"),
                    spelling,
                    blockers,
                );
            }
            visit_type(
                owner,
                &format!("{path}.return_type"),
                &function.return_type,
                false,
                required,
                queue,
                blockers,
            );
            for (index, parameter) in function.parameters.iter().enumerate() {
                visit_type(
                    owner,
                    &format!("{path}.parameters[{index}]"),
                    &parameter.ty,
                    false,
                    required,
                    queue,
                    blockers,
                );
            }
        }
        CTypeKind::AliasRef(id) => enqueue(
            required,
            queue,
            *id,
            if behind_pointer {
                Need::OpaqueSufficient
            } else {
                Need::Definition
            },
        ),
        CTypeKind::EnumRef(id) => enqueue(required, queue, *id, Need::Definition),
        CTypeKind::RecordRef(id) => enqueue(
            required,
            queue,
            *id,
            if behind_pointer {
                Need::OpaqueSufficient
            } else {
                Need::Definition
            },
        ),
        CTypeKind::Unsupported { spelling, .. } => reject(owner, path, spelling, blockers),
        CTypeKind::Void
        | CTypeKind::Bool
        | CTypeKind::Integer(_)
        | CTypeKind::Floating(_)
        | CTypeKind::Complex(_) => {}
    }
}

fn enqueue(
    required: &mut BTreeMap<DeclarationId, Need>,
    queue: &mut VecDeque<(DeclarationId, Need)>,
    id: DeclarationId,
    need: Need,
) {
    match required.get_mut(&id) {
        None => {
            required.insert(id, need);
            queue.push_back((id, need));
        }
        Some(existing) if *existing < need => {
            *existing = need;
            queue.push_back((id, need));
        }
        Some(_) => {}
    }
}

fn reject_status(
    owner: DeclarationId,
    path: &str,
    status: &SupportStatus,
    blockers: &mut Vec<CompletionBlocker>,
) {
    match status {
        SupportStatus::Supported => {}
        SupportStatus::Partial { reason, .. } | SupportStatus::Unsupported { reason, .. } => {
            reject(owner, path, reason, blockers);
        }
    }
}

fn reject_attributes(
    owner: DeclarationId,
    path: &str,
    attributes: &[super::SourceAttribute],
    blockers: &mut Vec<CompletionBlocker>,
) {
    for (index, attribute) in attributes.iter().enumerate() {
        if attribute.disposition == AttributeDisposition::UnsupportedAbiRelevant {
            reject(
                owner,
                format!("{path}[{index}]"),
                &attribute.spelling,
                blockers,
            );
        }
    }
}

fn reject(
    id: DeclarationId,
    path: impl Into<String>,
    reason: impl Into<String>,
    blockers: &mut Vec<CompletionBlocker>,
) {
    blockers.push(CompletionBlocker::Unsupported {
        id,
        path: path.into(),
        reason: reason.into(),
    });
}
