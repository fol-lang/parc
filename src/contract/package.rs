use super::{
    validate::{validate_source_package, ValidationLimits},
    Completeness, ContractViolation, EffectiveSourceInputs, SchemaHeader, SourceDeclaration,
    SourceDiagnostic, SourceFile, SourceFingerprint, SourceMacro, TargetFingerprint, TargetSpec,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePackage {
    schema: SchemaHeader,
    target: TargetSpec,
    files: Vec<SourceFile>,
    inputs: EffectiveSourceInputs,
    declarations: Vec<SourceDeclaration>,
    macros: Vec<SourceMacro>,
    diagnostics: Vec<SourceDiagnostic>,
    completeness: Completeness,
    fingerprint: SourceFingerprint,
}

/// Unfingerprinted inputs accepted by [`SourcePackage::try_new`]. The schema
/// header and source fingerprint are derived by the constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePackageInput {
    pub target: TargetSpec,
    pub files: Vec<SourceFile>,
    pub inputs: EffectiveSourceInputs,
    pub declarations: Vec<SourceDeclaration>,
    pub macros: Vec<SourceMacro>,
    /// Strict canonical domain-field order; duplicate diagnostics are invalid.
    pub diagnostics: Vec<SourceDiagnostic>,
    pub completeness: Completeness,
}

#[derive(Debug, Error)]
pub enum SourcePackageBuildError {
    #[error("source package has {count} contract violation(s)", count = .violations.len())]
    Contract { violations: Vec<ContractViolation> },
    #[error("could not canonicalize source package: {0}")]
    Canonical(#[source] serde_json::Error),
}

impl SourcePackageBuildError {
    pub fn contract_violations(&self) -> Option<&[ContractViolation]> {
        match self {
            Self::Contract { violations } => Some(violations),
            Self::Canonical(_) => None,
        }
    }
}

pub(crate) struct SourcePackageParts {
    pub(crate) schema: SchemaHeader,
    pub(crate) target: TargetSpec,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) inputs: EffectiveSourceInputs,
    pub(crate) declarations: Vec<SourceDeclaration>,
    pub(crate) macros: Vec<SourceMacro>,
    pub(crate) diagnostics: Vec<SourceDiagnostic>,
    pub(crate) completeness: Completeness,
    pub(crate) fingerprint: SourceFingerprint,
}

impl SourcePackage {
    /// Validates all source invariants and derives the canonical schema-v2
    /// fingerprint. The returned package cannot be mutated in place.
    pub fn try_new(input: SourcePackageInput) -> Result<Self, SourcePackageBuildError> {
        let mut package = Self {
            schema: SchemaHeader::source_package_v2(),
            target: input.target,
            files: input.files,
            inputs: input.inputs,
            declarations: input.declarations,
            macros: input.macros,
            diagnostics: input.diagnostics,
            completeness: input.completeness,
            fingerprint: SourceFingerprint::derive(b"unfingerprinted-source-package"),
        };
        validate_source_package(&package, construction_limits())
            .map_err(|violations| SourcePackageBuildError::Contract { violations })?;
        package.fingerprint = super::codec::source_fingerprint(&package)
            .map_err(SourcePackageBuildError::Canonical)?;
        Ok(package)
    }

    pub(crate) fn from_parts(parts: SourcePackageParts) -> Self {
        Self {
            schema: parts.schema,
            target: parts.target,
            files: parts.files,
            inputs: parts.inputs,
            declarations: parts.declarations,
            macros: parts.macros,
            diagnostics: parts.diagnostics,
            completeness: parts.completeness,
            fingerprint: parts.fingerprint,
        }
    }

    pub fn schema(&self) -> &SchemaHeader {
        &self.schema
    }

    pub fn target(&self) -> &TargetSpec {
        &self.target
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn inputs(&self) -> &EffectiveSourceInputs {
        &self.inputs
    }

    pub fn declarations(&self) -> &[SourceDeclaration] {
        &self.declarations
    }

    pub fn macros(&self) -> &[SourceMacro] {
        &self.macros
    }

    pub fn diagnostics(&self) -> &[SourceDiagnostic] {
        &self.diagnostics
    }

    pub fn completeness(&self) -> &Completeness {
        &self.completeness
    }

    pub const fn fingerprint(&self) -> SourceFingerprint {
        self.fingerprint
    }

    pub const fn target_fingerprint(&self) -> TargetFingerprint {
        self.target.fingerprint()
    }

    pub fn declaration(&self, id: DeclarationId) -> Option<&SourceDeclaration> {
        self.declarations
            .binary_search_by_key(&id, |declaration| declaration.id)
            .ok()
            .map(|index| &self.declarations[index])
    }

    /// Package-aware selection membership. `AllSupported` includes only
    /// declarations whose checked support state is fully supported.
    pub fn selection_contains(&self, selection: &super::Selection, id: DeclarationId) -> bool {
        match selection {
            super::Selection::AllSupported => self
                .declaration(id)
                .is_some_and(|declaration| declaration.support.is_supported()),
            super::Selection::Only(ids) | super::Selection::OpaqueOnly(ids) => {
                ids.binary_search(&id).is_ok()
            }
        }
    }
}

use super::DeclarationId;

const fn construction_limits() -> ValidationLimits {
    ValidationLimits {
        files: 16_384,
        declarations: 1_000_000,
        macros: 1_000_000,
        diagnostics: 1_000_000,
        type_depth: 64,
    }
}
