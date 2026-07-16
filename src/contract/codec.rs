//! Strict schema-v2 transport codec.

use serde_json::Error as JsonError;
use serde_json::Value;
use thiserror::Error;

use super::{
    validate::{validate_source_package, ValidationLimits},
    wire::v2::{
        canonical_payload_bytes, RawSourcePackageEnvelope, SourcePackageEnvelope, SourcePackageWire,
    },
    ContractViolation, SchemaHeader, SourceFingerprint, SourcePackage, TargetValidationError,
    SOURCE_PACKAGE_KIND, SOURCE_PACKAGE_SCHEMA_ID, SOURCE_PACKAGE_SCHEMA_VERSION,
};

/// Resource limits applied before a decoded value can become a domain value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_bytes: usize,
    pub max_files: usize,
    pub max_declarations: usize,
    pub max_macros: usize,
    pub max_diagnostics: usize,
    pub max_type_depth: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_files: 16_384,
            max_declarations: 1_000_000,
            max_macros: 1_000_000,
            max_diagnostics: 1_000_000,
            max_type_depth: 64,
        }
    }
}

impl DecodeLimits {
    pub(crate) const fn validation_limits(self) -> ValidationLimits {
        ValidationLimits {
            files: self.max_files,
            declarations: self.max_declarations,
            macros: self.max_macros,
            diagnostics: self.max_diagnostics,
            type_depth: self.max_type_depth,
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("source-package envelope is {actual} bytes; decoder limit is {maximum}")]
    ByteLimit { actual: usize, maximum: usize },
    #[error("malformed source-package envelope: {0}")]
    Envelope(#[source] JsonError),
    #[error("unexpected artifact kind {found:?}; expected {SOURCE_PACKAGE_KIND:?}")]
    Kind { found: String },
    #[error("unsupported schema id {found:?}; expected {SOURCE_PACKAGE_SCHEMA_ID:?}")]
    SchemaId { found: String },
    #[error(
        "unsupported source-package schema version {found}; only version {SOURCE_PACKAGE_SCHEMA_VERSION} is accepted"
    )]
    SchemaVersion { found: u32 },
    #[error("malformed schema-v2 source-package payload: {0}")]
    Payload(#[source] JsonError),
    #[error("unknown field {field:?} in strict unit variant at {path}")]
    UnknownUnitVariantField { path: String, field: String },
    #[error("payload schema header does not exactly match the envelope header")]
    PayloadSchema,
    #[error("invalid target specification: {0}")]
    Target(#[source] TargetValidationError),
    #[error("source fingerprint differs between envelope and payload")]
    EnvelopeFingerprint,
    #[error("source fingerprint mismatch: stored {stored}, recomputed {recomputed}")]
    SourceFingerprint {
        stored: SourceFingerprint,
        recomputed: SourceFingerprint,
    },
    #[error("source package has {count} contract violation(s)", count = .violations.len())]
    Contract { violations: Vec<ContractViolation> },
    #[error("could not canonicalize schema-v2 source package: {0}")]
    Canonical(#[source] JsonError),
}

impl DecodeError {
    pub fn contract_violations(&self) -> Option<&[ContractViolation]> {
        match self {
            Self::Contract { violations } => Some(violations),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("source package has {count} contract violation(s)", count = .violations.len())]
    Contract { violations: Vec<ContractViolation> },
    #[error("source fingerprint mismatch: stored {stored}, recomputed {recomputed}")]
    SourceFingerprint {
        stored: SourceFingerprint,
        recomputed: SourceFingerprint,
    },
    #[error("could not serialize canonical schema-v2 source package: {0}")]
    Serialization(#[source] JsonError),
}

impl EncodeError {
    pub fn contract_violations(&self) -> Option<&[ContractViolation]> {
        match self {
            Self::Contract { violations } => Some(violations),
            _ => None,
        }
    }
}

/// Decodes only the current schema-v2 envelope and returns a checked domain
/// value. There is deliberately no v1 or best-effort fallback.
pub fn decode_source_package(bytes: &[u8]) -> Result<SourcePackage, DecodeError> {
    decode_source_package_with_limits(bytes, DecodeLimits::default())
}

pub fn decode_source_package_with_limits(
    bytes: &[u8],
    limits: DecodeLimits,
) -> Result<SourcePackage, DecodeError> {
    if bytes.len() > limits.max_bytes {
        return Err(DecodeError::ByteLimit {
            actual: bytes.len(),
            maximum: limits.max_bytes,
        });
    }

    // RawValue keeps the payload opaque until kind and schema have been
    // accepted. This prevents a future or unrelated shape from reaching the
    // domain decoder.
    let envelope: RawSourcePackageEnvelope =
        serde_json::from_slice(bytes).map_err(DecodeError::Envelope)?;
    validate_envelope_header(&envelope.kind, &envelope.schema)?;

    let payload_value: Value =
        serde_json::from_str(envelope.payload.get()).map_err(DecodeError::Payload)?;
    reject_unknown_unit_variant_fields(&payload_value, "payload")?;
    let wire: SourcePackageWire =
        serde_json::from_str(envelope.payload.get()).map_err(DecodeError::Payload)?;
    if wire.schema != envelope.schema {
        return Err(DecodeError::PayloadSchema);
    }
    if wire.fingerprint != envelope.fingerprint {
        return Err(DecodeError::EnvelopeFingerprint);
    }

    let package = wire.into_domain().map_err(DecodeError::Target)?;
    validate_source_package(&package, limits.validation_limits())
        .map_err(|violations| DecodeError::Contract { violations })?;

    let recomputed = source_fingerprint(&package).map_err(DecodeError::Canonical)?;
    if package.fingerprint() != recomputed {
        return Err(DecodeError::SourceFingerprint {
            stored: package.fingerprint(),
            recomputed,
        });
    }
    Ok(package)
}

fn reject_unknown_unit_variant_fields(value: &Value, path: &str) -> Result<(), DecodeError> {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                reject_unknown_unit_variant_fields(value, &format!("{path}[{index}]"))?;
            }
        }
        Value::Object(object) => {
            for (tag, variant) in [
                ("status", object.get("status")),
                ("state", object.get("state")),
                ("policy", object.get("policy")),
                ("kind", object.get("kind")),
                ("format", object.get("format")),
                ("class", object.get("class")),
            ] {
                let Some(variant) = variant.and_then(Value::as_str) else {
                    continue;
                };
                if is_unit_variant(tag, variant) && object.len() != 1 {
                    let field = object
                        .keys()
                        .find(|field| field.as_str() != tag)
                        .cloned()
                        .unwrap_or_else(|| tag.to_owned());
                    return Err(DecodeError::UnknownUnitVariantField {
                        path: path.to_owned(),
                        field,
                    });
                }
            }
            for (field, value) in object {
                reject_unknown_unit_variant_fields(value, &format!("{path}.{field}"))?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn is_unit_variant(tag: &str, variant: &str) -> bool {
    match tag {
        "status" => variant == "supported",
        "state" => variant == "complete",
        "policy" => variant == "hermetic",
        "kind" => matches!(
            variant,
            "translation_unit"
                | "void"
                | "bool"
                | "float"
                | "double"
                | "long_double"
                | "float128"
                | "incomplete"
                | "flexible"
                | "unspecified_parameters"
                | "c"
                | "cdecl"
                | "stdcall"
                | "fastcall"
                | "vectorcall"
                | "thiscall"
                | "sys_v64"
                | "win64"
                | "aapcs"
        ),
        "format" => matches!(
            variant,
            "ieee_binary32"
                | "ieee_binary64"
                | "ieee_binary128"
                | "x87_extended80"
                | "ibm_double_double128"
                | "decimal32"
                | "decimal64"
                | "decimal128"
        ),
        "class" => matches!(
            variant,
            "ilp32" | "lp64" | "llp64" | "ilp64" | "i_l_p32" | "l_p64" | "l_l_p64" | "i_l_p64"
        ),
        _ => false,
    }
}

/// Produces the unique minified schema-v2 JSON representation.
pub fn encode_source_package(package: &SourcePackage) -> Result<Vec<u8>, EncodeError> {
    validate_source_package(package, DecodeLimits::default().validation_limits())
        .map_err(|violations| EncodeError::Contract { violations })?;
    let recomputed = source_fingerprint(package).map_err(EncodeError::Serialization)?;
    if package.fingerprint() != recomputed {
        return Err(EncodeError::SourceFingerprint {
            stored: package.fingerprint(),
            recomputed,
        });
    }

    let mut encoded = serde_json::to_vec(&SourcePackageEnvelope {
        kind: SOURCE_PACKAGE_KIND,
        schema: package.schema(),
        fingerprint: package.fingerprint(),
        payload: SourcePackageWire::from_domain(package),
    })
    .map_err(EncodeError::Serialization)?;
    encoded.push(b'\n');
    Ok(encoded)
}

pub(crate) fn source_fingerprint(package: &SourcePackage) -> Result<SourceFingerprint, JsonError> {
    canonical_payload_bytes(package).map(|bytes| SourceFingerprint::derive(&bytes))
}

fn validate_envelope_header(kind: &str, schema: &SchemaHeader) -> Result<(), DecodeError> {
    if kind != SOURCE_PACKAGE_KIND {
        return Err(DecodeError::Kind {
            found: kind.to_owned(),
        });
    }
    if schema.id != SOURCE_PACKAGE_SCHEMA_ID {
        return Err(DecodeError::SchemaId {
            found: schema.id.clone(),
        });
    }
    if schema.version != SOURCE_PACKAGE_SCHEMA_VERSION {
        return Err(DecodeError::SchemaVersion {
            found: schema.version,
        });
    }
    Ok(())
}
