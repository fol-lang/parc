//! Private schema-v2 data-transfer objects.
//!
//! Keeping these types private lets the typed API evolve without making
//! ordinary serde construction a way to forge a checked domain value.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::super::{
    Completeness, EffectiveSourceInputs, SchemaHeader, SourceDeclaration, SourceDiagnostic,
    SourceFile, SourceFingerprint, SourceMacro, SourcePackage, TargetFingerprint, TargetSpec,
    TargetSpecParts,
};
use crate::contract::package::SourcePackageParts;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSourcePackageEnvelope {
    pub kind: String,
    pub schema: SchemaHeader,
    pub fingerprint: SourceFingerprint,
    pub payload: Box<RawValue>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePackageEnvelope<'a> {
    pub kind: &'static str,
    pub schema: &'a SchemaHeader,
    pub fingerprint: SourceFingerprint,
    pub payload: SourcePackageWire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetSpecWire {
    pub spec: TargetSpecParts,
    pub fingerprint: TargetFingerprint,
}

impl TargetSpecWire {
    pub(crate) fn from_domain(target: &TargetSpec) -> Self {
        Self {
            spec: target.parts(),
            fingerprint: target.fingerprint(),
        }
    }

    pub(crate) fn into_domain(self) -> Result<TargetSpec, super::super::TargetValidationError> {
        TargetSpec::try_from_parts_with_fingerprint(self.spec, self.fingerprint)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourcePackageWire {
    pub schema: SchemaHeader,
    pub target: TargetSpecWire,
    pub files: Vec<SourceFile>,
    pub inputs: EffectiveSourceInputs,
    pub declarations: Vec<SourceDeclaration>,
    pub macros: Vec<SourceMacro>,
    pub diagnostics: Vec<SourceDiagnostic>,
    pub completeness: Completeness,
    pub fingerprint: SourceFingerprint,
}

impl SourcePackageWire {
    pub(crate) fn from_domain(package: &SourcePackage) -> Self {
        Self {
            schema: package.schema().clone(),
            target: TargetSpecWire::from_domain(package.target()),
            files: package.files().to_vec(),
            inputs: package.inputs().clone(),
            declarations: package.declarations().to_vec(),
            macros: package.macros().to_vec(),
            diagnostics: package.diagnostics().to_vec(),
            completeness: package.completeness().clone(),
            fingerprint: package.fingerprint(),
        }
    }

    pub(crate) fn into_domain(self) -> Result<SourcePackage, super::super::TargetValidationError> {
        let target = self.target.into_domain()?;
        Ok(SourcePackage::from_parts(SourcePackageParts {
            schema: self.schema,
            target,
            files: self.files,
            inputs: self.inputs,
            declarations: self.declarations,
            macros: self.macros,
            diagnostics: self.diagnostics,
            completeness: self.completeness,
            fingerprint: self.fingerprint,
        }))
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct FingerprintPayload<'a> {
    schema: &'a SchemaHeader,
    target: TargetSpecWire,
    files: &'a [SourceFile],
    inputs: &'a EffectiveSourceInputs,
    declarations: &'a [SourceDeclaration],
    macros: &'a [SourceMacro],
    diagnostics: &'a [SourceDiagnostic],
    completeness: &'a Completeness,
}

/// Returns the canonical schema-v2 payload bytes used by the source
/// fingerprint. The cached source fingerprint itself is deliberately absent.
pub(crate) fn canonical_payload_bytes(
    package: &SourcePackage,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&FingerprintPayload {
        schema: package.schema(),
        target: TargetSpecWire::from_domain(package.target()),
        files: package.files(),
        inputs: package.inputs(),
        declarations: package.declarations(),
        macros: package.macros(),
        diagnostics: package.diagnostics(),
        completeness: package.completeness(),
    })
}
