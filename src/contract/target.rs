//! Checked compiler, target, and C data-model identity.
//!
//! A [`TargetSpec`] is deliberately not deserializable or directly
//! constructible. Callers build [`TargetSpecParts`] from explicit toolchain
//! evidence and pass it to [`TargetSpec::try_new`]. This keeps downstream
//! stages from accidentally treating a guessed host configuration as an ABI
//! identity.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use super::fingerprint::{ContentFingerprint, TargetFingerprint};
use super::types::Signedness;

const TARGET_CANONICAL_FORMAT: &str = "follang.parc.target-canonical.v1";

/// A fully checked target identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    triple: String,
    architecture: Architecture,
    vendor: Vendor,
    operating_system: OperatingSystem,
    environment: Environment,
    object_format: ObjectFormat,
    endian: Endian,
    pointer_width: u16,
    c_data_model: CDataModel,
    language_standard: LanguageStandard,
    extension_profile: ExtensionProfile,
    compiler: CompilerIdentity,
    sysroot: Option<SysrootIdentity>,
    abi_flags: Vec<NormalizedCompilerArg>,
    fingerprint: TargetFingerprint,
}

/// Explicit, unfingerprinted input to [`TargetSpec::try_new`].
///
/// The public fields make configuration assembly straightforward without
/// making the resulting checked [`TargetSpec`] mutable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetSpecParts {
    pub triple: String,
    pub architecture: Architecture,
    pub vendor: Vendor,
    pub operating_system: OperatingSystem,
    pub environment: Environment,
    pub object_format: ObjectFormat,
    pub endian: Endian,
    pub pointer_width: u16,
    pub c_data_model: CDataModel,
    pub language_standard: LanguageStandard,
    pub extension_profile: ExtensionProfile,
    pub compiler: CompilerIdentity,
    pub sysroot: Option<SysrootIdentity>,
    /// ABI-affecting arguments in command-line order. Repetition is semantic
    /// and is therefore preserved.
    pub abi_flags: Vec<NormalizedCompilerArg>,
}

impl TargetSpec {
    /// Validates explicit target facts, derives the target fingerprint, and
    /// returns an immutable target identity.
    pub fn try_new(parts: TargetSpecParts) -> Result<Self, TargetValidationError> {
        parts.validate()?;
        let fingerprint = TargetFingerprint::derive(&canonical_parts_bytes(&parts));
        Ok(Self::from_parts_unchecked(parts, fingerprint))
    }

    /// Reconstructs a target from a checked wire fingerprint.
    ///
    /// This is crate-private because public callers must not supply their own
    /// cached fingerprint. The codec uses it after parsing the schema-v2 DTO.
    pub(crate) fn try_from_parts_with_fingerprint(
        parts: TargetSpecParts,
        fingerprint: TargetFingerprint,
    ) -> Result<Self, TargetValidationError> {
        let mut violations = collect_violations(&parts);
        let expected = TargetFingerprint::derive(&canonical_parts_bytes(&parts));
        if fingerprint != expected {
            violations.push(TargetViolation::TargetFingerprintMismatch {
                stored: fingerprint.to_string(),
                recomputed: expected.to_string(),
            });
        }
        if !violations.is_empty() {
            return Err(TargetValidationError::new(violations));
        }
        Ok(Self::from_parts_unchecked(parts, fingerprint))
    }

    fn from_parts_unchecked(parts: TargetSpecParts, fingerprint: TargetFingerprint) -> Self {
        Self {
            triple: parts.triple,
            architecture: parts.architecture,
            vendor: parts.vendor,
            operating_system: parts.operating_system,
            environment: parts.environment,
            object_format: parts.object_format,
            endian: parts.endian,
            pointer_width: parts.pointer_width,
            c_data_model: parts.c_data_model,
            language_standard: parts.language_standard,
            extension_profile: parts.extension_profile,
            compiler: parts.compiler,
            sysroot: parts.sysroot,
            abi_flags: parts.abi_flags,
            fingerprint,
        }
    }

    /// Re-runs every target invariant, including the cached fingerprint.
    pub fn validate(&self) -> Result<(), TargetValidationError> {
        let parts = self.parts();
        let mut violations = collect_violations(&parts);
        let expected = TargetFingerprint::derive(&canonical_parts_bytes(&parts));
        if self.fingerprint != expected {
            violations.push(TargetViolation::TargetFingerprintMismatch {
                stored: self.fingerprint.to_string(),
                recomputed: expected.to_string(),
            });
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(TargetValidationError::new(violations))
        }
    }

    pub fn triple(&self) -> &str {
        &self.triple
    }

    pub fn architecture(&self) -> Architecture {
        self.architecture
    }

    pub fn vendor(&self) -> &Vendor {
        &self.vendor
    }

    pub fn operating_system(&self) -> OperatingSystem {
        self.operating_system
    }

    pub fn environment(&self) -> Environment {
        self.environment
    }

    pub fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    pub fn endian(&self) -> Endian {
        self.endian
    }

    pub fn pointer_width(&self) -> u16 {
        self.pointer_width
    }

    pub fn c_data_model(&self) -> &CDataModel {
        &self.c_data_model
    }

    pub fn language_standard(&self) -> LanguageStandard {
        self.language_standard
    }

    pub fn extension_profile(&self) -> &ExtensionProfile {
        &self.extension_profile
    }

    pub fn compiler(&self) -> &CompilerIdentity {
        &self.compiler
    }

    pub fn sysroot(&self) -> Option<&SysrootIdentity> {
        self.sysroot.as_ref()
    }

    pub fn abi_flags(&self) -> &[NormalizedCompilerArg] {
        &self.abi_flags
    }

    pub const fn fingerprint(&self) -> TargetFingerprint {
        self.fingerprint
    }

    pub(crate) fn parts(&self) -> TargetSpecParts {
        TargetSpecParts {
            triple: self.triple.clone(),
            architecture: self.architecture,
            vendor: self.vendor.clone(),
            operating_system: self.operating_system,
            environment: self.environment,
            object_format: self.object_format,
            endian: self.endian,
            pointer_width: self.pointer_width,
            c_data_model: self.c_data_model.clone(),
            language_standard: self.language_standard,
            extension_profile: self.extension_profile.clone(),
            compiler: self.compiler.clone(),
            sysroot: self.sysroot.clone(),
            abi_flags: self.abi_flags.clone(),
        }
    }
}

impl TargetSpecParts {
    /// Validates all target, compiler, and C data-model facts together.
    pub fn validate(&self) -> Result<(), TargetValidationError> {
        let violations = collect_violations(self);
        if violations.is_empty() {
            Ok(())
        } else {
            Err(TargetValidationError::new(violations))
        }
    }
}

/// Architectures intentionally supported by the contract. There is no
/// catch-all variant: an unrecognized architecture is a construction error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86,
    X86_64,
    Arm,
    Aarch64,
    RiscV32,
    RiscV64,
    PowerPc,
    PowerPc64,
    S390x,
    Mips,
    Mips64,
    Sparc64,
    Wasm32,
    Wasm64,
}

/// Explicit normalized vendor component of a target triple.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Vendor(String);

impl Vendor {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TargetValueError> {
        let value = value.into();
        validate_component("vendor", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Vendor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Linux,
    Android,
    Darwin,
    MacOs,
    Ios,
    TvOs,
    WatchOs,
    Windows,
    FreeBsd,
    NetBsd,
    OpenBsd,
    DragonFly,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    None,
    Gnu,
    GnuAbi64,
    Musl,
    Msvc,
    Android,
    Eabi,
    Eabihf,
    Simulator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectFormat {
    Elf,
    MachO,
    Coff,
    Wasm,
    Xcoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageStandard {
    C89,
    C95,
    C99,
    C11,
    C17,
    C23,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionProfile {
    pub family: ExtensionFamily,
    /// Canonical set: normalized, sorted, and unique.
    pub enabled: Vec<ExtensionId>,
}

impl ExtensionProfile {
    /// Creates the canonical set representation for enabled extensions.
    pub fn new(family: ExtensionFamily, enabled: impl IntoIterator<Item = ExtensionId>) -> Self {
        let mut enabled: Vec<_> = enabled.into_iter().collect();
        enabled.sort();
        enabled.dedup();
        Self { family, enabled }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionFamily {
    Strict,
    Gnu,
    Clang,
    Msvc,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtensionId(String);

impl ExtensionId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TargetValueError> {
        let value = value.into();
        validate_normalized_identifier("extension id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExtensionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompilerFamily {
    Gcc,
    Clang,
    AppleClang,
    Msvc,
}

/// Stable identity of the compiler which produced the effective source view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilerIdentity {
    family: CompilerFamily,
    logical_executable: String,
    executable_content: ContentFingerprint,
    version_text: ContentFingerprint,
    reported_target: String,
    version: String,
}

impl CompilerIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        family: CompilerFamily,
        logical_executable: impl Into<String>,
        executable_content: ContentFingerprint,
        version_text: ContentFingerprint,
        reported_target: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, TargetValueError> {
        let logical_executable = logical_executable.into();
        validate_logical_path("compiler logical executable", &logical_executable)?;
        let reported_target = reported_target.into();
        validate_target_text("compiler reported target", &reported_target)?;
        let version = version.into();
        validate_human_text("compiler version", &version)?;
        Ok(Self {
            family,
            logical_executable,
            executable_content,
            version_text,
            reported_target,
            version,
        })
    }

    pub fn family(&self) -> CompilerFamily {
        self.family
    }

    pub fn logical_executable(&self) -> &str {
        &self.logical_executable
    }

    pub fn executable_content(&self) -> ContentFingerprint {
        self.executable_content
    }

    pub fn version_text(&self) -> ContentFingerprint {
        self.version_text
    }

    pub fn reported_target(&self) -> &str {
        &self.reported_target
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Stable identity of the compiler's effective sysroot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SysrootIdentity {
    logical_path: String,
    content: ContentFingerprint,
}

impl SysrootIdentity {
    pub fn try_new(
        logical_path: impl Into<String>,
        content: ContentFingerprint,
    ) -> Result<Self, TargetValueError> {
        let logical_path = logical_path.into();
        validate_logical_path("sysroot logical path", &logical_path)?;
        Ok(Self {
            logical_path,
            content,
        })
    }

    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    pub fn content(&self) -> ContentFingerprint {
        self.content
    }
}

/// One normalized ABI-affecting compiler argument.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NormalizedCompilerArg(String);

impl NormalizedCompilerArg {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TargetValueError> {
        let value = value.into();
        validate_compiler_arg(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NormalizedCompilerArg {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CDataModel {
    pub class: CDataModelClass,
    pub char_bit: u16,
    pub char_signedness: CharSignedness,
    pub signed_integer_representation: SignedIntegerRepresentation,

    pub bool_layout: ScalarLayout,
    pub char_layout: ScalarLayout,
    pub short_layout: ScalarLayout,
    pub int_layout: ScalarLayout,
    pub long_layout: ScalarLayout,
    pub long_long_layout: ScalarLayout,
    pub int128_layout: Option<ScalarLayout>,
    pub pointer_layout: ScalarLayout,

    pub float_layout: FloatingLayout,
    pub double_layout: FloatingLayout,
    pub long_double_layout: FloatingLayout,

    pub wchar_layout: IntegerLayout,
    pub size_t_layout: IntegerLayout,
    pub ptrdiff_t_layout: IntegerLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "class", content = "name", rename_all = "snake_case")]
pub enum CDataModelClass {
    ILP32,
    LP64,
    LLP64,
    ILP64,
    Explicit(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScalarLayout {
    pub storage_bits: u16,
    pub alignment_bits: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegerLayout {
    pub scalar: ScalarLayout,
    pub signedness: Signedness,
    pub representation: SignedIntegerRepresentation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloatingLayout {
    pub scalar: ScalarLayout,
    pub format: FloatingFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case", deny_unknown_fields)]
pub enum FloatingFormat {
    IeeeBinary32,
    IeeeBinary64,
    IeeeBinary128,
    X87Extended80,
    IbmDoubleDouble128,
    Decimal32,
    Decimal64,
    Decimal128,
    Explicit {
        name: String,
        radix: u16,
        precision_bits: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CharSignedness {
    Signed,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignedIntegerRepresentation {
    TwosComplement,
    OnesComplement,
    SignMagnitude,
}

/// Failure while constructing one normalized target value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TargetValueError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} must be NFC-normalized")]
    NotNfc { field: &'static str },
    #[error("{field} must not have leading or trailing whitespace")]
    SurroundingWhitespace { field: &'static str },
    #[error("{field} contains a control or NUL character at byte {index}")]
    ControlCharacter { field: &'static str, index: usize },
    #[error("{field} contains invalid character {character:?} at byte {index}")]
    InvalidCharacter {
        field: &'static str,
        character: char,
        index: usize,
    },
    #[error("{field} must be a relative logical path")]
    AbsolutePath { field: &'static str },
    #[error("{field} contains forbidden path component {component:?}")]
    ForbiddenPathComponent {
        field: &'static str,
        component: String,
    },
}

/// One independently reportable target invariant failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TargetViolation {
    #[error("invalid target triple: {detail}")]
    InvalidTriple { detail: String },
    #[error("target triple architecture component {component:?} does not match {declared:?}")]
    TripleArchitectureMismatch {
        component: String,
        declared: Architecture,
    },
    #[error("target triple vendor component {component:?} does not match {declared:?}")]
    TripleVendorMismatch { component: String, declared: String },
    #[error("target triple OS component {component:?} does not match {declared:?}")]
    TripleOperatingSystemMismatch {
        component: String,
        declared: OperatingSystem,
    },
    #[error("target triple environment component {component:?} does not match {declared:?}")]
    TripleEnvironmentMismatch {
        component: String,
        declared: Environment,
    },
    #[error(
        "target triple architecture component {component:?} implies {implied:?} endian, not {declared:?}"
    )]
    TripleEndianMismatch {
        component: String,
        implied: Endian,
        declared: Endian,
    },
    #[error(
        "target triple architecture component {component:?} implies {implied}-bit pointers, not {declared}"
    )]
    TriplePointerWidthMismatch {
        component: String,
        implied: u16,
        declared: u16,
    },
    #[error(
        "target pointer width {pointer_width} does not match C pointer storage width {storage_bits}"
    )]
    PointerWidthMismatch {
        pointer_width: u16,
        storage_bits: u16,
    },
    #[error("invalid {field} layout: {detail}")]
    InvalidLayout { field: &'static str, detail: String },
    #[error("C data-model class {class:?} requires {field}={expected}, found {actual}")]
    DataModelClassMismatch {
        class: CDataModelClass,
        field: &'static str,
        expected: u16,
        actual: u16,
    },
    #[error("invalid C data-model fact {field}: {detail}")]
    InvalidDataModelFact { field: &'static str, detail: String },
    #[error("floating layout {field} with format {format:?}: {detail}")]
    FloatingFormatMismatch {
        field: &'static str,
        format: FloatingFormat,
        detail: String,
    },
    #[error("unsupported target combination: {detail}")]
    UnsupportedTargetCombination { detail: String },
    #[error("invalid extension profile: {detail}")]
    InvalidExtensionProfile { detail: String },
    #[error("invalid compiler identity field {field}: {detail}")]
    InvalidCompilerIdentity { field: &'static str, detail: String },
    #[error("compiler reported target {reported:?} does not match requested target {requested:?}")]
    CompilerReportedTargetMismatch { reported: String, requested: String },
    #[error("invalid sysroot identity: {detail}")]
    InvalidSysrootIdentity { detail: String },
    #[error("invalid ABI compiler argument at index {index}: {detail}")]
    InvalidAbiFlag { index: usize, detail: String },
    #[error("target fingerprint mismatch: stored {stored}, recomputed {recomputed}")]
    TargetFingerprintMismatch { stored: String, recomputed: String },
}

/// Aggregated target validation failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("target specification has {count} violation(s)", count = .violations.len())]
pub struct TargetValidationError {
    violations: Vec<TargetViolation>,
}

impl TargetValidationError {
    fn new(violations: Vec<TargetViolation>) -> Self {
        debug_assert!(!violations.is_empty());
        Self { violations }
    }

    pub fn violations(&self) -> &[TargetViolation] {
        &self.violations
    }

    pub fn into_violations(self) -> Vec<TargetViolation> {
        self.violations
    }
}

fn collect_violations(parts: &TargetSpecParts) -> Vec<TargetViolation> {
    let mut violations = Vec::new();

    let parsed = match parse_target_triple(&parts.triple) {
        Ok(parsed) => Some(parsed),
        Err(detail) => {
            violations.push(TargetViolation::InvalidTriple { detail });
            None
        }
    };

    if let Some(parsed) = parsed.as_ref() {
        if parsed.architecture != parts.architecture {
            violations.push(TargetViolation::TripleArchitectureMismatch {
                component: parsed.architecture_component.to_owned(),
                declared: parts.architecture,
            });
        }
        if parsed.vendor != parts.vendor.as_str() {
            violations.push(TargetViolation::TripleVendorMismatch {
                component: parsed.vendor.to_owned(),
                declared: parts.vendor.as_str().to_owned(),
            });
        }
        if parsed.operating_system != parts.operating_system {
            violations.push(TargetViolation::TripleOperatingSystemMismatch {
                component: parsed.operating_system_component.to_owned(),
                declared: parts.operating_system,
            });
        }
        if parsed.environment != parts.environment {
            violations.push(TargetViolation::TripleEnvironmentMismatch {
                component: parsed.environment_component.unwrap_or("").to_owned(),
                declared: parts.environment,
            });
        }
        if parsed.endian != parts.endian {
            violations.push(TargetViolation::TripleEndianMismatch {
                component: parsed.architecture_component.to_owned(),
                implied: parsed.endian,
                declared: parts.endian,
            });
        }
        if parsed.pointer_width != parts.pointer_width {
            violations.push(TargetViolation::TriplePointerWidthMismatch {
                component: parsed.architecture_component.to_owned(),
                implied: parsed.pointer_width,
                declared: parts.pointer_width,
            });
        }

        validate_target_combination(parts, parsed, &mut violations);
    }

    if parts.pointer_width != parts.c_data_model.pointer_layout.storage_bits {
        violations.push(TargetViolation::PointerWidthMismatch {
            pointer_width: parts.pointer_width,
            storage_bits: parts.c_data_model.pointer_layout.storage_bits,
        });
    }

    validate_c_data_model(&parts.c_data_model, &mut violations);
    validate_extension_profile(&parts.extension_profile, &mut violations);
    validate_compiler_identity(&parts.compiler, &parts.triple, &mut violations);

    if let Some(sysroot) = &parts.sysroot {
        if let Err(error) = validate_logical_path("sysroot logical path", &sysroot.logical_path) {
            violations.push(TargetViolation::InvalidSysrootIdentity {
                detail: error.to_string(),
            });
        }
    }

    for (index, flag) in parts.abi_flags.iter().enumerate() {
        if let Err(error) = validate_compiler_arg(flag.as_str()) {
            violations.push(TargetViolation::InvalidAbiFlag {
                index,
                detail: error.to_string(),
            });
        }
    }

    violations
}

fn validate_target_combination(
    parts: &TargetSpecParts,
    parsed: &ParsedTargetTriple<'_>,
    violations: &mut Vec<TargetViolation>,
) {
    if !environment_is_supported(parts.operating_system, parts.environment) {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "environment {:?} is not valid for {:?}",
                parts.environment, parts.operating_system
            ),
        });
    }

    if !architecture_os_is_supported(parts.architecture, parts.operating_system) {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "architecture {:?} is not supported for {:?}",
                parts.architecture, parts.operating_system
            ),
        });
    }

    let object_format_matches = match parts.object_format {
        ObjectFormat::Wasm => matches!(
            parts.architecture,
            Architecture::Wasm32 | Architecture::Wasm64
        ),
        ObjectFormat::MachO => is_apple_os(parts.operating_system),
        ObjectFormat::Coff => parts.operating_system == OperatingSystem::Windows,
        ObjectFormat::Xcoff => {
            matches!(
                parts.architecture,
                Architecture::PowerPc | Architecture::PowerPc64
            ) && parts.vendor.as_str() == "ibm"
                && parts.operating_system == OperatingSystem::None
        }
        ObjectFormat::Elf => {
            !matches!(
                parts.architecture,
                Architecture::Wasm32 | Architecture::Wasm64
            ) && !is_apple_os(parts.operating_system)
                && parts.operating_system != OperatingSystem::Windows
        }
    };
    if !object_format_matches {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "object format {:?} is incompatible with {:?}/{:?}",
                parts.object_format, parts.architecture, parts.operating_system
            ),
        });
    }

    if is_apple_os(parts.operating_system) && parts.vendor.as_str() != "apple" {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "Apple operating system {:?} requires vendor apple, found {}",
                parts.operating_system, parts.vendor
            ),
        });
    }

    if matches!(
        parts.architecture,
        Architecture::Wasm32 | Architecture::Wasm64
    ) && parts.object_format != ObjectFormat::Wasm
    {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: "WebAssembly architectures require the wasm object format".to_owned(),
        });
    }
    if !matches!(
        parts.architecture,
        Architecture::Wasm32 | Architecture::Wasm64
    ) && parts.object_format == ObjectFormat::Wasm
    {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: "the wasm object format requires a WebAssembly architecture".to_owned(),
        });
    }

    // Keep the parsed facts in this validation lane so changes to alias
    // handling cannot accidentally bypass combination checks.
    if parsed.architecture == Architecture::S390x && parsed.endian != Endian::Big {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: "s390x is only supported as big-endian".to_owned(),
        });
    }

    let compiler_combination_is_supported = match parts.compiler.family {
        CompilerFamily::Msvc => {
            parts.operating_system == OperatingSystem::Windows
                && parts.environment == Environment::Msvc
        }
        CompilerFamily::AppleClang => is_apple_os(parts.operating_system),
        CompilerFamily::Gcc => parts.environment != Environment::Msvc,
        CompilerFamily::Clang => true,
    };
    if !compiler_combination_is_supported {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "compiler family {:?} is incompatible with {:?}/{:?}",
                parts.compiler.family, parts.operating_system, parts.environment
            ),
        });
    }
    if parts.environment == Environment::Msvc
        && !matches!(
            parts.compiler.family,
            CompilerFamily::Msvc | CompilerFamily::Clang
        )
    {
        violations.push(TargetViolation::UnsupportedTargetCombination {
            detail: format!(
                "MSVC environment requires an MSVC-compatible compiler, found {:?}",
                parts.compiler.family
            ),
        });
    }
}

fn environment_is_supported(os: OperatingSystem, environment: Environment) -> bool {
    match os {
        OperatingSystem::Linux => matches!(
            environment,
            Environment::None
                | Environment::Gnu
                | Environment::GnuAbi64
                | Environment::Musl
                | Environment::Android
        ),
        OperatingSystem::Android => {
            matches!(environment, Environment::None | Environment::Android)
        }
        OperatingSystem::Darwin | OperatingSystem::MacOs => environment == Environment::None,
        OperatingSystem::Ios | OperatingSystem::TvOs | OperatingSystem::WatchOs => {
            matches!(environment, Environment::None | Environment::Simulator)
        }
        OperatingSystem::Windows => {
            matches!(
                environment,
                Environment::None | Environment::Gnu | Environment::Msvc
            )
        }
        OperatingSystem::FreeBsd
        | OperatingSystem::NetBsd
        | OperatingSystem::OpenBsd
        | OperatingSystem::DragonFly => environment == Environment::None,
        OperatingSystem::None => matches!(
            environment,
            Environment::None | Environment::Eabi | Environment::Eabihf
        ),
    }
}

fn architecture_os_is_supported(
    architecture: Architecture,
    operating_system: OperatingSystem,
) -> bool {
    match operating_system {
        OperatingSystem::Linux => {
            !matches!(architecture, Architecture::Wasm32 | Architecture::Wasm64)
        }
        OperatingSystem::Android => matches!(
            architecture,
            Architecture::X86
                | Architecture::X86_64
                | Architecture::Arm
                | Architecture::Aarch64
                | Architecture::RiscV64
        ),
        OperatingSystem::Darwin | OperatingSystem::MacOs => matches!(
            architecture,
            Architecture::X86 | Architecture::X86_64 | Architecture::Aarch64
        ),
        OperatingSystem::Ios | OperatingSystem::TvOs | OperatingSystem::WatchOs => {
            matches!(
                architecture,
                Architecture::X86
                    | Architecture::X86_64
                    | Architecture::Arm
                    | Architecture::Aarch64
            )
        }
        OperatingSystem::Windows => matches!(
            architecture,
            Architecture::X86 | Architecture::X86_64 | Architecture::Arm | Architecture::Aarch64
        ),
        OperatingSystem::FreeBsd
        | OperatingSystem::NetBsd
        | OperatingSystem::OpenBsd
        | OperatingSystem::DragonFly => !matches!(
            architecture,
            Architecture::Wasm32 | Architecture::Wasm64 | Architecture::S390x
        ),
        OperatingSystem::None => true,
    }
}

fn is_apple_os(os: OperatingSystem) -> bool {
    matches!(
        os,
        OperatingSystem::Darwin
            | OperatingSystem::MacOs
            | OperatingSystem::Ios
            | OperatingSystem::TvOs
            | OperatingSystem::WatchOs
    )
}

fn validate_c_data_model(model: &CDataModel, violations: &mut Vec<TargetViolation>) {
    if model.char_bit == 0 {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "char_bit",
            detail: "must be nonzero".to_owned(),
        });
    }

    validate_scalar_layout("bool", model.bool_layout, model.char_bit, violations);
    validate_scalar_layout("char", model.char_layout, model.char_bit, violations);
    validate_scalar_layout("short", model.short_layout, model.char_bit, violations);
    validate_scalar_layout("int", model.int_layout, model.char_bit, violations);
    validate_scalar_layout("long", model.long_layout, model.char_bit, violations);
    validate_scalar_layout(
        "long_long",
        model.long_long_layout,
        model.char_bit,
        violations,
    );
    if let Some(layout) = model.int128_layout {
        validate_scalar_layout("int128", layout, model.char_bit, violations);
        if layout.storage_bits != 128 {
            violations.push(TargetViolation::InvalidDataModelFact {
                field: "int128.storage_bits",
                detail: format!("must be 128, found {}", layout.storage_bits),
            });
        }
    }
    validate_scalar_layout("pointer", model.pointer_layout, model.char_bit, violations);
    validate_floating_layout("float", &model.float_layout, model.char_bit, violations);
    validate_floating_layout("double", &model.double_layout, model.char_bit, violations);
    validate_floating_layout(
        "long_double",
        &model.long_double_layout,
        model.char_bit,
        violations,
    );
    validate_scalar_layout(
        "wchar",
        model.wchar_layout.scalar,
        model.char_bit,
        violations,
    );
    validate_scalar_layout(
        "size_t",
        model.size_t_layout.scalar,
        model.char_bit,
        violations,
    );
    validate_scalar_layout(
        "ptrdiff_t",
        model.ptrdiff_t_layout.scalar,
        model.char_bit,
        violations,
    );

    if model.char_layout.storage_bits != model.char_bit {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "char.storage_bits",
            detail: format!(
                "must equal char_bit {}, found {}",
                model.char_bit, model.char_layout.storage_bits
            ),
        });
    }

    let ordered_integer_widths = [
        ("short", model.short_layout.storage_bits),
        ("int", model.int_layout.storage_bits),
        ("long", model.long_layout.storage_bits),
        ("long_long", model.long_long_layout.storage_bits),
    ];
    for pair in ordered_integer_widths.windows(2) {
        let (left_name, left_width) = pair[0];
        let (right_name, right_width) = pair[1];
        if left_width > right_width {
            violations.push(TargetViolation::InvalidDataModelFact {
                field: "integer rank widths",
                detail: format!(
                    "{left_name} width {left_width} exceeds {right_name} width {right_width}"
                ),
            });
        }
    }

    if model.size_t_layout.scalar.storage_bits != model.pointer_layout.storage_bits {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "size_t.storage_bits",
            detail: format!(
                "must match pointer width {}, found {}",
                model.pointer_layout.storage_bits, model.size_t_layout.scalar.storage_bits
            ),
        });
    }
    if model.ptrdiff_t_layout.scalar.storage_bits != model.pointer_layout.storage_bits {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "ptrdiff_t.storage_bits",
            detail: format!(
                "must match pointer width {}, found {}",
                model.pointer_layout.storage_bits, model.ptrdiff_t_layout.scalar.storage_bits
            ),
        });
    }
    if model.size_t_layout.signedness != Signedness::Unsigned {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "size_t.signedness",
            detail: "must be unsigned".to_owned(),
        });
    }
    if model.ptrdiff_t_layout.signedness != Signedness::Signed {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "ptrdiff_t.signedness",
            detail: "must be signed".to_owned(),
        });
    }
    if model.ptrdiff_t_layout.representation != model.signed_integer_representation {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "ptrdiff_t.representation",
            detail: "must match the target signed integer representation".to_owned(),
        });
    }
    if model.wchar_layout.signedness == Signedness::Signed
        && model.wchar_layout.representation != model.signed_integer_representation
    {
        violations.push(TargetViolation::InvalidDataModelFact {
            field: "wchar.representation",
            detail: "signed wchar_t must match the target signed integer representation".to_owned(),
        });
    }

    validate_data_model_class(model, violations);
}

fn validate_scalar_layout(
    field: &'static str,
    layout: ScalarLayout,
    char_bit: u16,
    violations: &mut Vec<TargetViolation>,
) {
    if layout.storage_bits == 0 {
        violations.push(TargetViolation::InvalidLayout {
            field,
            detail: "storage width must be nonzero".to_owned(),
        });
    }
    if layout.alignment_bits == 0 {
        violations.push(TargetViolation::InvalidLayout {
            field,
            detail: "alignment must be nonzero".to_owned(),
        });
    } else if !layout.alignment_bits.is_power_of_two() {
        violations.push(TargetViolation::InvalidLayout {
            field,
            detail: format!("alignment {} is not a power of two", layout.alignment_bits),
        });
    }
    if char_bit != 0 && !layout.storage_bits.is_multiple_of(char_bit) {
        violations.push(TargetViolation::InvalidLayout {
            field,
            detail: format!(
                "storage width {} is not a whole number of {}-bit C bytes",
                layout.storage_bits, char_bit
            ),
        });
    }
    if char_bit != 0 && !layout.alignment_bits.is_multiple_of(char_bit) {
        violations.push(TargetViolation::InvalidLayout {
            field,
            detail: format!(
                "alignment {} is not a whole number of {}-bit C bytes",
                layout.alignment_bits, char_bit
            ),
        });
    }
}

fn validate_floating_layout(
    field: &'static str,
    layout: &FloatingLayout,
    char_bit: u16,
    violations: &mut Vec<TargetViolation>,
) {
    validate_scalar_layout(field, layout.scalar, char_bit, violations);
    let storage = layout.scalar.storage_bits;
    let valid_storage = match &layout.format {
        FloatingFormat::IeeeBinary32 | FloatingFormat::Decimal32 => storage == 32,
        FloatingFormat::IeeeBinary64 | FloatingFormat::Decimal64 => storage == 64,
        FloatingFormat::IeeeBinary128
        | FloatingFormat::IbmDoubleDouble128
        | FloatingFormat::Decimal128 => storage == 128,
        // x87 values have 80 significant storage bits but common ABIs place
        // them in 80-, 96-, or 128-bit scalar storage slots.
        FloatingFormat::X87Extended80 => matches!(storage, 80 | 96 | 128),
        FloatingFormat::Explicit {
            name,
            radix,
            precision_bits,
        } => {
            if let Err(error) = validate_normalized_identifier("floating format name", name) {
                violations.push(TargetViolation::FloatingFormatMismatch {
                    field,
                    format: layout.format.clone(),
                    detail: error.to_string(),
                });
            }
            if *radix < 2 {
                violations.push(TargetViolation::FloatingFormatMismatch {
                    field,
                    format: layout.format.clone(),
                    detail: format!("radix must be at least 2, found {radix}"),
                });
            }
            if *precision_bits == 0 || *precision_bits > storage {
                violations.push(TargetViolation::FloatingFormatMismatch {
                    field,
                    format: layout.format.clone(),
                    detail: format!("precision must be in 1..={storage}, found {precision_bits}"),
                });
            }
            true
        }
    };
    if !valid_storage {
        violations.push(TargetViolation::FloatingFormatMismatch {
            field,
            format: layout.format.clone(),
            detail: format!("incompatible storage width {storage}"),
        });
    }
}

fn validate_data_model_class(model: &CDataModel, violations: &mut Vec<TargetViolation>) {
    match &model.class {
        CDataModelClass::ILP32 => {
            require_model_width(model, "int", model.int_layout.storage_bits, 32, violations);
            require_model_width(
                model,
                "long",
                model.long_layout.storage_bits,
                32,
                violations,
            );
            require_model_width(
                model,
                "pointer",
                model.pointer_layout.storage_bits,
                32,
                violations,
            );
        }
        CDataModelClass::LP64 => {
            require_model_width(model, "int", model.int_layout.storage_bits, 32, violations);
            require_model_width(
                model,
                "long",
                model.long_layout.storage_bits,
                64,
                violations,
            );
            require_model_width(
                model,
                "pointer",
                model.pointer_layout.storage_bits,
                64,
                violations,
            );
        }
        CDataModelClass::LLP64 => {
            require_model_width(model, "int", model.int_layout.storage_bits, 32, violations);
            require_model_width(
                model,
                "long",
                model.long_layout.storage_bits,
                32,
                violations,
            );
            require_model_width(
                model,
                "long_long",
                model.long_long_layout.storage_bits,
                64,
                violations,
            );
            require_model_width(
                model,
                "pointer",
                model.pointer_layout.storage_bits,
                64,
                violations,
            );
        }
        CDataModelClass::ILP64 => {
            require_model_width(model, "int", model.int_layout.storage_bits, 64, violations);
            require_model_width(
                model,
                "long",
                model.long_layout.storage_bits,
                64,
                violations,
            );
            require_model_width(
                model,
                "pointer",
                model.pointer_layout.storage_bits,
                64,
                violations,
            );
        }
        CDataModelClass::Explicit(name) => {
            if let Err(error) = validate_normalized_identifier("explicit data-model name", name) {
                violations.push(TargetViolation::InvalidDataModelFact {
                    field: "class",
                    detail: error.to_string(),
                });
            }
        }
    }
}

fn require_model_width(
    model: &CDataModel,
    field: &'static str,
    actual: u16,
    expected: u16,
    violations: &mut Vec<TargetViolation>,
) {
    if actual != expected {
        violations.push(TargetViolation::DataModelClassMismatch {
            class: model.class.clone(),
            field,
            expected,
            actual,
        });
    }
}

fn validate_extension_profile(profile: &ExtensionProfile, violations: &mut Vec<TargetViolation>) {
    for extension in &profile.enabled {
        if let Err(error) = validate_normalized_identifier("extension id", extension.as_str()) {
            violations.push(TargetViolation::InvalidExtensionProfile {
                detail: error.to_string(),
            });
        }
    }
    for pair in profile.enabled.windows(2) {
        if pair[0] >= pair[1] {
            violations.push(TargetViolation::InvalidExtensionProfile {
                detail: format!(
                    "extension IDs must be strictly sorted and unique; found {} before {}",
                    pair[0], pair[1]
                ),
            });
        }
    }
}

fn validate_compiler_identity(
    compiler: &CompilerIdentity,
    requested_target: &str,
    violations: &mut Vec<TargetViolation>,
) {
    if let Err(error) =
        validate_logical_path("compiler logical executable", compiler.logical_executable())
    {
        violations.push(TargetViolation::InvalidCompilerIdentity {
            field: "logical_executable",
            detail: error.to_string(),
        });
    }
    if let Err(error) = validate_target_text("compiler reported target", compiler.reported_target())
    {
        violations.push(TargetViolation::InvalidCompilerIdentity {
            field: "reported_target",
            detail: error.to_string(),
        });
    }
    if let Err(error) = validate_human_text("compiler version", compiler.version()) {
        violations.push(TargetViolation::InvalidCompilerIdentity {
            field: "version",
            detail: error.to_string(),
        });
    }
    if compiler.reported_target() != requested_target {
        violations.push(TargetViolation::CompilerReportedTargetMismatch {
            reported: compiler.reported_target().to_owned(),
            requested: requested_target.to_owned(),
        });
    }
}

struct ParsedTargetTriple<'a> {
    architecture_component: &'a str,
    architecture: Architecture,
    vendor: &'a str,
    operating_system_component: &'a str,
    operating_system: OperatingSystem,
    environment_component: Option<&'a str>,
    environment: Environment,
    endian: Endian,
    pointer_width: u16,
}

fn parse_target_triple(triple: &str) -> Result<ParsedTargetTriple<'_>, String> {
    validate_target_text("target triple", triple).map_err(|error| error.to_string())?;
    let components: Vec<_> = triple.split('-').collect();
    if !matches!(components.len(), 3 | 4) {
        return Err(format!(
            "expected architecture-vendor-os[-environment], found {} components",
            components.len()
        ));
    }

    let (architecture, endian, pointer_width) = parse_architecture(components[0])
        .ok_or_else(|| format!("unsupported architecture component {:?}", components[0]))?;
    validate_component("target vendor", components[1]).map_err(|error| error.to_string())?;
    let operating_system = parse_operating_system(components[2])
        .ok_or_else(|| format!("unsupported operating-system component {:?}", components[2]))?;
    let (environment_component, environment) = if components.len() == 4 {
        let component = components[3];
        let environment = parse_environment(component)
            .ok_or_else(|| format!("unsupported environment component {component:?}"))?;
        (Some(component), environment)
    } else {
        (None, Environment::None)
    };

    Ok(ParsedTargetTriple {
        architecture_component: components[0],
        architecture,
        vendor: components[1],
        operating_system_component: components[2],
        operating_system,
        environment_component,
        environment,
        endian,
        pointer_width,
    })
}

fn parse_architecture(component: &str) -> Option<(Architecture, Endian, u16)> {
    let parsed = match component {
        "x86" | "i386" | "i486" | "i586" | "i686" => (Architecture::X86, Endian::Little, 32),
        "x86_64" => (Architecture::X86_64, Endian::Little, 64),
        "arm" | "thumb" => (Architecture::Arm, Endian::Little, 32),
        "armeb" | "thumbeb" => (Architecture::Arm, Endian::Big, 32),
        "aarch64" => (Architecture::Aarch64, Endian::Little, 64),
        "aarch64_be" => (Architecture::Aarch64, Endian::Big, 64),
        "powerpc" | "ppc" => (Architecture::PowerPc, Endian::Big, 32),
        "powerpcle" | "ppcle" => (Architecture::PowerPc, Endian::Little, 32),
        "powerpc64" | "ppc64" => (Architecture::PowerPc64, Endian::Big, 64),
        "powerpc64le" | "ppc64le" => (Architecture::PowerPc64, Endian::Little, 64),
        "s390x" => (Architecture::S390x, Endian::Big, 64),
        "mips" => (Architecture::Mips, Endian::Big, 32),
        "mipsel" => (Architecture::Mips, Endian::Little, 32),
        "mips64" => (Architecture::Mips64, Endian::Big, 64),
        "mips64el" => (Architecture::Mips64, Endian::Little, 64),
        "sparc64" => (Architecture::Sparc64, Endian::Big, 64),
        "wasm32" => (Architecture::Wasm32, Endian::Little, 32),
        "wasm64" => (Architecture::Wasm64, Endian::Little, 64),
        _ if component.starts_with("armv") || component.starts_with("thumbv") => {
            (Architecture::Arm, Endian::Little, 32)
        }
        _ if component.starts_with("riscv32") => (Architecture::RiscV32, Endian::Little, 32),
        _ if component.starts_with("riscv64") => (Architecture::RiscV64, Endian::Little, 64),
        _ => return None,
    };
    Some(parsed)
}

fn parse_operating_system(component: &str) -> Option<OperatingSystem> {
    match component {
        "linux" => Some(OperatingSystem::Linux),
        "android" => Some(OperatingSystem::Android),
        "darwin" => Some(OperatingSystem::Darwin),
        "macos" => Some(OperatingSystem::MacOs),
        "ios" => Some(OperatingSystem::Ios),
        "tvos" => Some(OperatingSystem::TvOs),
        "watchos" => Some(OperatingSystem::WatchOs),
        "windows" => Some(OperatingSystem::Windows),
        "freebsd" => Some(OperatingSystem::FreeBsd),
        "netbsd" => Some(OperatingSystem::NetBsd),
        "openbsd" => Some(OperatingSystem::OpenBsd),
        "dragonfly" => Some(OperatingSystem::DragonFly),
        "none" => Some(OperatingSystem::None),
        _ => None,
    }
}

fn parse_environment(component: &str) -> Option<Environment> {
    match component {
        "gnu" => Some(Environment::Gnu),
        "gnuabi64" => Some(Environment::GnuAbi64),
        "musl" => Some(Environment::Musl),
        "msvc" => Some(Environment::Msvc),
        "android" => Some(Environment::Android),
        "eabi" => Some(Environment::Eabi),
        "eabihf" => Some(Environment::Eabihf),
        "sim" | "simulator" => Some(Environment::Simulator),
        _ => None,
    }
}

fn validate_component(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    validate_nfc_nonempty(field, value)?;
    for (index, character) in value.char_indices() {
        if !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_') {
            return Err(TargetValueError::InvalidCharacter {
                field,
                character,
                index,
            });
        }
    }
    Ok(())
}

fn validate_normalized_identifier(
    field: &'static str,
    value: &str,
) -> Result<(), TargetValueError> {
    validate_nfc_nonempty(field, value)?;
    for (index, character) in value.char_indices() {
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | '-' | '.' | '+'))
        {
            return Err(TargetValueError::InvalidCharacter {
                field,
                character,
                index,
            });
        }
    }
    Ok(())
}

fn validate_target_text(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    validate_nfc_nonempty(field, value)?;
    for (index, character) in value.char_indices() {
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | '-'))
        {
            return Err(TargetValueError::InvalidCharacter {
                field,
                character,
                index,
            });
        }
    }
    if value.split('-').any(str::is_empty) {
        return Err(TargetValueError::ForbiddenPathComponent {
            field,
            component: String::new(),
        });
    }
    Ok(())
}

fn validate_human_text(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    validate_nfc_nonempty(field, value)?;
    if value.trim() != value {
        return Err(TargetValueError::SurroundingWhitespace { field });
    }
    validate_no_controls(field, value)
}

fn validate_compiler_arg(value: &str) -> Result<(), TargetValueError> {
    let field = "normalized compiler argument";
    validate_nfc_nonempty(field, value)?;
    if value.trim() != value {
        return Err(TargetValueError::SurroundingWhitespace { field });
    }
    validate_no_controls(field, value)
}

fn validate_logical_path(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    validate_nfc_nonempty(field, value)?;
    validate_no_controls(field, value)?;
    if value.starts_with('/') || value.starts_with('\\') {
        return Err(TargetValueError::AbsolutePath { field });
    }
    if value.contains('\\') {
        let index = value.find('\\').expect("contains was checked");
        return Err(TargetValueError::InvalidCharacter {
            field,
            character: '\\',
            index,
        });
    }
    for component in value.split('/') {
        if component.is_empty() || matches!(component, "." | "..") {
            return Err(TargetValueError::ForbiddenPathComponent {
                field,
                component: component.to_owned(),
            });
        }
    }
    // Reject Windows drive-relative forms such as C:tool even on Unix hosts.
    if value.as_bytes().get(1) == Some(&b':')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
    {
        return Err(TargetValueError::AbsolutePath { field });
    }
    Ok(())
}

fn validate_nfc_nonempty(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    if value.is_empty() {
        return Err(TargetValueError::Empty { field });
    }
    if value.nfc().collect::<String>() != value {
        return Err(TargetValueError::NotNfc { field });
    }
    Ok(())
}

fn validate_no_controls(field: &'static str, value: &str) -> Result<(), TargetValueError> {
    if let Some((index, _)) = value
        .char_indices()
        .find(|(_, character)| character.is_control())
    {
        return Err(TargetValueError::ControlCharacter { field, index });
    }
    Ok(())
}

fn canonical_parts_bytes(parts: &TargetSpecParts) -> Vec<u8> {
    let mut encoder = TargetCanonicalEncoder::new();
    encoder.text(TARGET_CANONICAL_FORMAT);
    encoder.text(&parts.triple);
    encoder.text(architecture_name(parts.architecture));
    encoder.text(parts.vendor.as_str());
    encoder.text(operating_system_name(parts.operating_system));
    encoder.text(environment_name(parts.environment));
    encoder.text(object_format_name(parts.object_format));
    encoder.text(endian_name(parts.endian));
    encoder.u16(parts.pointer_width);
    encode_c_data_model(&mut encoder, &parts.c_data_model);
    encoder.text(language_standard_name(parts.language_standard));
    encoder.text(extension_family_name(parts.extension_profile.family));
    encoder.count(parts.extension_profile.enabled.len());
    for extension in &parts.extension_profile.enabled {
        encoder.text(extension.as_str());
    }
    encoder.text(compiler_family_name(parts.compiler.family));
    encoder.text(&parts.compiler.logical_executable);
    encoder.bytes(parts.compiler.executable_content.as_bytes());
    encoder.bytes(parts.compiler.version_text.as_bytes());
    encoder.text(&parts.compiler.reported_target);
    encoder.text(&parts.compiler.version);
    match &parts.sysroot {
        Some(sysroot) => {
            encoder.text("some");
            encoder.text(&sysroot.logical_path);
            encoder.bytes(sysroot.content.as_bytes());
        }
        None => encoder.text("none"),
    }
    encoder.count(parts.abi_flags.len());
    for flag in &parts.abi_flags {
        encoder.text(flag.as_str());
    }
    encoder.finish()
}

fn encode_c_data_model(encoder: &mut TargetCanonicalEncoder, model: &CDataModel) {
    match &model.class {
        CDataModelClass::ILP32 => encoder.text("ilp32"),
        CDataModelClass::LP64 => encoder.text("lp64"),
        CDataModelClass::LLP64 => encoder.text("llp64"),
        CDataModelClass::ILP64 => encoder.text("ilp64"),
        CDataModelClass::Explicit(name) => {
            encoder.text("explicit");
            encoder.text(name);
        }
    }
    encoder.u16(model.char_bit);
    encoder.text(char_signedness_name(model.char_signedness));
    encoder.text(signed_representation_name(
        model.signed_integer_representation,
    ));
    encoder.scalar(model.bool_layout);
    encoder.scalar(model.char_layout);
    encoder.scalar(model.short_layout);
    encoder.scalar(model.int_layout);
    encoder.scalar(model.long_layout);
    encoder.scalar(model.long_long_layout);
    match model.int128_layout {
        Some(layout) => {
            encoder.text("some");
            encoder.scalar(layout);
        }
        None => encoder.text("none"),
    }
    encoder.scalar(model.pointer_layout);
    encode_floating_layout(encoder, &model.float_layout);
    encode_floating_layout(encoder, &model.double_layout);
    encode_floating_layout(encoder, &model.long_double_layout);
    encode_integer_layout(encoder, model.wchar_layout);
    encode_integer_layout(encoder, model.size_t_layout);
    encode_integer_layout(encoder, model.ptrdiff_t_layout);
}

fn encode_floating_layout(encoder: &mut TargetCanonicalEncoder, layout: &FloatingLayout) {
    encoder.scalar(layout.scalar);
    match &layout.format {
        FloatingFormat::IeeeBinary32 => encoder.text("ieee_binary32"),
        FloatingFormat::IeeeBinary64 => encoder.text("ieee_binary64"),
        FloatingFormat::IeeeBinary128 => encoder.text("ieee_binary128"),
        FloatingFormat::X87Extended80 => encoder.text("x87_extended80"),
        FloatingFormat::IbmDoubleDouble128 => encoder.text("ibm_double_double128"),
        FloatingFormat::Decimal32 => encoder.text("decimal32"),
        FloatingFormat::Decimal64 => encoder.text("decimal64"),
        FloatingFormat::Decimal128 => encoder.text("decimal128"),
        FloatingFormat::Explicit {
            name,
            radix,
            precision_bits,
        } => {
            encoder.text("explicit");
            encoder.text(name);
            encoder.u16(*radix);
            encoder.u16(*precision_bits);
        }
    }
}

fn encode_integer_layout(encoder: &mut TargetCanonicalEncoder, layout: IntegerLayout) {
    encoder.scalar(layout.scalar);
    encoder.text(signedness_name(layout.signedness));
    encoder.text(signed_representation_name(layout.representation));
}

struct TargetCanonicalEncoder {
    bytes: Vec<u8>,
}

impl TargetCanonicalEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        let length = u64::try_from(value.len()).expect("target field exceeds u64");
        self.bytes.extend_from_slice(&length.to_le_bytes());
        self.bytes.extend_from_slice(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }

    fn count(&mut self, value: usize) {
        let value = u64::try_from(value).expect("target vector length exceeds u64");
        self.bytes(&value.to_le_bytes());
    }

    fn scalar(&mut self, layout: ScalarLayout) {
        self.u16(layout.storage_bits);
        self.u16(layout.alignment_bits);
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn architecture_name(value: Architecture) -> &'static str {
    match value {
        Architecture::X86 => "x86",
        Architecture::X86_64 => "x86_64",
        Architecture::Arm => "arm",
        Architecture::Aarch64 => "aarch64",
        Architecture::RiscV32 => "riscv32",
        Architecture::RiscV64 => "riscv64",
        Architecture::PowerPc => "power_pc",
        Architecture::PowerPc64 => "power_pc64",
        Architecture::S390x => "s390x",
        Architecture::Mips => "mips",
        Architecture::Mips64 => "mips64",
        Architecture::Sparc64 => "sparc64",
        Architecture::Wasm32 => "wasm32",
        Architecture::Wasm64 => "wasm64",
    }
}

fn operating_system_name(value: OperatingSystem) -> &'static str {
    match value {
        OperatingSystem::Linux => "linux",
        OperatingSystem::Android => "android",
        OperatingSystem::Darwin => "darwin",
        OperatingSystem::MacOs => "mac_os",
        OperatingSystem::Ios => "ios",
        OperatingSystem::TvOs => "tv_os",
        OperatingSystem::WatchOs => "watch_os",
        OperatingSystem::Windows => "windows",
        OperatingSystem::FreeBsd => "free_bsd",
        OperatingSystem::NetBsd => "net_bsd",
        OperatingSystem::OpenBsd => "open_bsd",
        OperatingSystem::DragonFly => "dragon_fly",
        OperatingSystem::None => "none",
    }
}

fn environment_name(value: Environment) -> &'static str {
    match value {
        Environment::None => "none",
        Environment::Gnu => "gnu",
        Environment::GnuAbi64 => "gnu_abi64",
        Environment::Musl => "musl",
        Environment::Msvc => "msvc",
        Environment::Android => "android",
        Environment::Eabi => "eabi",
        Environment::Eabihf => "eabihf",
        Environment::Simulator => "simulator",
    }
}

fn object_format_name(value: ObjectFormat) -> &'static str {
    match value {
        ObjectFormat::Elf => "elf",
        ObjectFormat::MachO => "mach_o",
        ObjectFormat::Coff => "coff",
        ObjectFormat::Wasm => "wasm",
        ObjectFormat::Xcoff => "xcoff",
    }
}

fn endian_name(value: Endian) -> &'static str {
    match value {
        Endian::Little => "little",
        Endian::Big => "big",
    }
}

fn language_standard_name(value: LanguageStandard) -> &'static str {
    match value {
        LanguageStandard::C89 => "c89",
        LanguageStandard::C95 => "c95",
        LanguageStandard::C99 => "c99",
        LanguageStandard::C11 => "c11",
        LanguageStandard::C17 => "c17",
        LanguageStandard::C23 => "c23",
    }
}

fn extension_family_name(value: ExtensionFamily) -> &'static str {
    match value {
        ExtensionFamily::Strict => "strict",
        ExtensionFamily::Gnu => "gnu",
        ExtensionFamily::Clang => "clang",
        ExtensionFamily::Msvc => "msvc",
    }
}

fn compiler_family_name(value: CompilerFamily) -> &'static str {
    match value {
        CompilerFamily::Gcc => "gcc",
        CompilerFamily::Clang => "clang",
        CompilerFamily::AppleClang => "apple_clang",
        CompilerFamily::Msvc => "msvc",
    }
}

fn char_signedness_name(value: CharSignedness) -> &'static str {
    match value {
        CharSignedness::Signed => "signed",
        CharSignedness::Unsigned => "unsigned",
    }
}

fn signedness_name(value: Signedness) -> &'static str {
    match value {
        Signedness::Signed => "signed",
        Signedness::Unsigned => "unsigned",
    }
}

fn signed_representation_name(value: SignedIntegerRepresentation) -> &'static str {
    match value {
        SignedIntegerRepresentation::TwosComplement => "twos_complement",
        SignedIntegerRepresentation::OnesComplement => "ones_complement",
        SignedIntegerRepresentation::SignMagnitude => "sign_magnitude",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(storage_bits: u16, alignment_bits: u16) -> ScalarLayout {
        ScalarLayout {
            storage_bits,
            alignment_bits,
        }
    }

    fn integer(storage_bits: u16, alignment_bits: u16, signedness: Signedness) -> IntegerLayout {
        IntegerLayout {
            scalar: scalar(storage_bits, alignment_bits),
            signedness,
            representation: SignedIntegerRepresentation::TwosComplement,
        }
    }

    fn lp64_data_model() -> CDataModel {
        CDataModel {
            class: CDataModelClass::LP64,
            char_bit: 8,
            char_signedness: CharSignedness::Signed,
            signed_integer_representation: SignedIntegerRepresentation::TwosComplement,
            bool_layout: scalar(8, 8),
            char_layout: scalar(8, 8),
            short_layout: scalar(16, 16),
            int_layout: scalar(32, 32),
            long_layout: scalar(64, 64),
            long_long_layout: scalar(64, 64),
            int128_layout: Some(scalar(128, 128)),
            pointer_layout: scalar(64, 64),
            float_layout: FloatingLayout {
                scalar: scalar(32, 32),
                format: FloatingFormat::IeeeBinary32,
            },
            double_layout: FloatingLayout {
                scalar: scalar(64, 64),
                format: FloatingFormat::IeeeBinary64,
            },
            long_double_layout: FloatingLayout {
                scalar: scalar(128, 128),
                format: FloatingFormat::X87Extended80,
            },
            wchar_layout: integer(32, 32, Signedness::Signed),
            size_t_layout: integer(64, 64, Signedness::Unsigned),
            ptrdiff_t_layout: integer(64, 64, Signedness::Signed),
        }
    }

    fn compiler(target: &str) -> CompilerIdentity {
        CompilerIdentity::try_new(
            CompilerFamily::Gcc,
            "toolchains/gcc/bin/gcc",
            ContentFingerprint::derive(b"gcc executable"),
            ContentFingerprint::derive(b"gcc version output"),
            target,
            "13.2.0",
        )
        .expect("valid compiler identity")
    }

    fn lp64_parts() -> TargetSpecParts {
        TargetSpecParts {
            triple: "x86_64-unknown-linux-gnu".to_owned(),
            architecture: Architecture::X86_64,
            vendor: Vendor::try_new("unknown").expect("valid vendor"),
            operating_system: OperatingSystem::Linux,
            environment: Environment::Gnu,
            object_format: ObjectFormat::Elf,
            endian: Endian::Little,
            pointer_width: 64,
            c_data_model: lp64_data_model(),
            language_standard: LanguageStandard::C17,
            extension_profile: ExtensionProfile::new(
                ExtensionFamily::Gnu,
                [
                    ExtensionId::try_new("vector-types").expect("valid extension"),
                    ExtensionId::try_new("attributes").expect("valid extension"),
                ],
            ),
            compiler: compiler("x86_64-unknown-linux-gnu"),
            sysroot: Some(
                SysrootIdentity::try_new(
                    "toolchains/gcc/sysroot",
                    ContentFingerprint::derive(b"sysroot"),
                )
                .expect("valid sysroot"),
            ),
            abi_flags: vec![
                NormalizedCompilerArg::try_new("-m64").expect("valid argument"),
                NormalizedCompilerArg::try_new("-fshort-enums").expect("valid argument"),
                NormalizedCompilerArg::try_new("-m64").expect("valid repeated argument"),
            ],
        }
    }

    #[test]
    fn checked_target_is_deterministic_and_accepts_x87_storage_slot() {
        let first = TargetSpec::try_new(lp64_parts()).expect("valid target");
        let second = TargetSpec::try_new(lp64_parts()).expect("valid target");
        assert_eq!(first.fingerprint(), second.fingerprint());
        assert_eq!(
            first.c_data_model().long_double_layout.format,
            FloatingFormat::X87Extended80
        );
        assert_eq!(
            first
                .abi_flags()
                .iter()
                .map(NormalizedCompilerArg::as_str)
                .collect::<Vec<_>>(),
            ["-m64", "-fshort-enums", "-m64"]
        );
    }

    #[test]
    fn ordered_repeated_abi_flags_affect_fingerprint() {
        let original = TargetSpec::try_new(lp64_parts()).expect("valid target");
        let mut reordered = lp64_parts();
        reordered.abi_flags.swap(0, 1);
        let reordered = TargetSpec::try_new(reordered).expect("valid target");
        assert_ne!(original.fingerprint(), reordered.fingerprint());

        let mut deduplicated = lp64_parts();
        deduplicated.abi_flags.pop();
        let deduplicated = TargetSpec::try_new(deduplicated).expect("valid target");
        assert_ne!(original.fingerprint(), deduplicated.fingerprint());
    }

    #[test]
    fn extension_profile_is_sorted_and_unique() {
        let profile = ExtensionProfile::new(
            ExtensionFamily::Gnu,
            [
                ExtensionId::try_new("z").expect("valid extension"),
                ExtensionId::try_new("a").expect("valid extension"),
                ExtensionId::try_new("z").expect("valid extension"),
            ],
        );
        assert_eq!(
            profile
                .enabled
                .iter()
                .map(ExtensionId::as_str)
                .collect::<Vec<_>>(),
            ["a", "z"]
        );
    }

    #[test]
    fn reports_independent_target_and_data_model_mismatches() {
        let mut parts = lp64_parts();
        parts.architecture = Architecture::Aarch64;
        parts.pointer_width = 32;
        parts.c_data_model.long_layout.storage_bits = 32;
        let error = TargetSpec::try_new(parts).expect_err("invalid target");
        assert!(error.violations().iter().any(|violation| matches!(
            violation,
            TargetViolation::TripleArchitectureMismatch { .. }
        )));
        assert!(error.violations().iter().any(|violation| matches!(
            violation,
            TargetViolation::TriplePointerWidthMismatch { .. }
        )));
        assert!(error.violations().iter().any(|violation| matches!(
            violation,
            TargetViolation::DataModelClassMismatch { field: "long", .. }
        )));
    }

    #[test]
    fn checked_wire_path_rejects_stale_fingerprint() {
        let parts = lp64_parts();
        let unrelated = TargetFingerprint::derive(b"not this target");
        let error = TargetSpec::try_from_parts_with_fingerprint(parts, unrelated)
            .expect_err("stale fingerprint");
        assert!(error.violations().iter().any(|violation| matches!(
            violation,
            TargetViolation::TargetFingerprintMismatch { .. }
        )));
    }

    #[test]
    fn logical_identities_reject_absolute_and_parent_paths() {
        let content = ContentFingerprint::derive(b"identity");
        assert!(SysrootIdentity::try_new("/usr", content).is_err());
        assert!(SysrootIdentity::try_new("toolchain/../usr", content).is_err());
        assert!(CompilerIdentity::try_new(
            CompilerFamily::Gcc,
            "C:\\gcc.exe",
            content,
            content,
            "x86_64-unknown-linux-gnu",
            "13.2.0",
        )
        .is_err());
    }
}
