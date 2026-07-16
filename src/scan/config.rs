//! Explicit, target-checked configuration for source scanning.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use thiserror::Error;

use crate::contract::{
    normalize_logical_path, ContentFingerprint, DefineEvent, ExtensionFamily, LanguageStandard,
    TargetSpec,
};
use crate::driver::Flavor;

const DEFAULT_GENERATED_PATH: &str = "__parc_generated__/translation-unit.i";

/// Producer resource ceilings. These are fail-closed execution policy, not
/// target facts, and therefore do not affect artifact identity unless a limit
/// is hit and produces a forcing diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanLimits {
    pub max_input_file_bytes: u64,
    pub max_total_input_bytes: u64,
    pub max_include_depth: usize,
    pub max_include_count: usize,
    pub max_macro_definitions: usize,
    pub max_macro_expansions: usize,
    /// Maximum number of simultaneously expanding macros. This bounds the
    /// implementation stack independently of the total expansion-work limit.
    pub max_macro_expansion_depth: usize,
    pub max_tokens: usize,
    pub max_generated_bytes: u64,
    pub external_timeout: Duration,
    pub max_external_output_bytes: u64,
}

impl ScanLimits {
    pub const fn production() -> Self {
        Self {
            max_input_file_bytes: 16 * 1024 * 1024,
            max_total_input_bytes: 64 * 1024 * 1024,
            max_include_depth: 128,
            max_include_count: 4_096,
            max_macro_definitions: 100_000,
            max_macro_expansions: 1_000_000,
            max_macro_expansion_depth: 256,
            max_tokens: 4_000_000,
            max_generated_bytes: 64 * 1024 * 1024,
            external_timeout: Duration::from_secs(30),
            max_external_output_bytes: 64 * 1024 * 1024,
        }
    }

    fn is_valid(&self) -> bool {
        let production = Self::production();
        self.max_input_file_bytes > 0
            && self.max_input_file_bytes <= production.max_input_file_bytes
            && self.max_total_input_bytes > 0
            && self.max_total_input_bytes <= production.max_total_input_bytes
            && self.max_include_depth > 0
            && self.max_include_depth <= production.max_include_depth
            && self.max_include_count > 0
            && self.max_include_count <= production.max_include_count
            && self.max_macro_definitions > 0
            && self.max_macro_definitions <= production.max_macro_definitions
            && self.max_macro_expansions > 0
            && self.max_macro_expansions <= production.max_macro_expansions
            && self.max_macro_expansion_depth > 0
            && self.max_macro_expansion_depth <= production.max_macro_expansion_depth
            && self.max_tokens > 0
            && self.max_tokens <= production.max_tokens
            && self.max_generated_bytes > 0
            && self.max_generated_bytes <= production.max_generated_bytes
            && !self.external_timeout.is_zero()
            && self.external_timeout <= production.external_timeout
            && self.max_external_output_bytes > 0
            && self.max_external_output_bytes <= production.max_external_output_bytes
    }
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self::production()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathMappingError {
    #[error("path mapping requires at least one physical-to-logical root")]
    Empty,
    #[error("physical root must be absolute and contain no parent traversal: {0}")]
    InvalidPhysicalRoot(String),
    #[error("logical path is not canonical: {0}")]
    InvalidLogicalPath(String),
    #[error("logical mapping root is duplicated: {0}")]
    DuplicateLogicalRoot(String),
    #[error("physical mapping root is duplicated after canonicalization: {0}")]
    DuplicatePhysicalRoot(String),
    #[error("path mapping rules overlap and could produce ambiguous identities: {0} and {1}")]
    AmbiguousRules(String, String),
    #[error("path is outside every configured mapping root: {0}")]
    UnmappedPath(String),
    #[error("could not canonicalize physical path {path}: {message}")]
    Canonicalize { path: String, message: String },
    #[error("physical path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMappingRule {
    physical_root: PathBuf,
    logical_root: String,
}

impl PathMappingRule {
    pub fn try_new(
        physical_root: impl Into<PathBuf>,
        logical_root: impl Into<String>,
    ) -> Result<Self, PathMappingError> {
        let supplied_root = physical_root.into();
        let physical_root = std::fs::canonicalize(&supplied_root).map_err(|error| {
            PathMappingError::Canonicalize {
                path: supplied_root.display().to_string(),
                message: error.to_string(),
            }
        })?;
        if !physical_root.is_absolute()
            || physical_root
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(PathMappingError::InvalidPhysicalRoot(
                physical_root.display().to_string(),
            ));
        }
        if physical_root.to_str().is_none() {
            return Err(PathMappingError::NonUtf8Path(
                "physical mapping root".to_owned(),
            ));
        }
        let supplied = logical_root.into();
        let logical_root = normalize_logical_path(&supplied)
            .map_err(|_| PathMappingError::InvalidLogicalPath(supplied.clone()))?;
        if logical_root != supplied {
            return Err(PathMappingError::InvalidLogicalPath(supplied));
        }
        Ok(Self {
            physical_root,
            logical_root,
        })
    }

    pub fn physical_root(&self) -> &Path {
        &self.physical_root
    }

    pub fn logical_root(&self) -> &str {
        &self.logical_root
    }
}

/// Canonical physical-to-logical path mapping used by every source identity.
///
/// Physical roots are operational and deliberately excluded from the mapping
/// fingerprint so relocating an otherwise identical source tree preserves IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMapping {
    rules: Vec<PathMappingRule>,
    generated_path: String,
    fingerprint: ContentFingerprint,
}

impl PathMapping {
    pub fn try_new(
        rules: impl IntoIterator<Item = PathMappingRule>,
    ) -> Result<Self, PathMappingError> {
        Self::try_new_with_generated_path(rules, DEFAULT_GENERATED_PATH)
    }

    pub fn try_new_with_generated_path(
        rules: impl IntoIterator<Item = PathMappingRule>,
        generated_path: impl Into<String>,
    ) -> Result<Self, PathMappingError> {
        let mut rules: Vec<_> = rules.into_iter().collect();
        if rules.is_empty() {
            return Err(PathMappingError::Empty);
        }
        let mut logical_roots = BTreeSet::new();
        let mut physical_roots = BTreeSet::new();
        for rule in &rules {
            if !logical_roots.insert(rule.logical_root.clone()) {
                return Err(PathMappingError::DuplicateLogicalRoot(
                    rule.logical_root.clone(),
                ));
            }
            if !physical_roots.insert(rule.physical_root.clone()) {
                return Err(PathMappingError::DuplicatePhysicalRoot(
                    rule.physical_root.display().to_string(),
                ));
            }
        }
        for (index, left) in rules.iter().enumerate() {
            for right in &rules[index + 1..] {
                let physical_overlap = left.physical_root.starts_with(&right.physical_root)
                    || right.physical_root.starts_with(&left.physical_root);
                let logical_overlap =
                    logical_root_contains(&left.logical_root, &right.logical_root)
                        || logical_root_contains(&right.logical_root, &left.logical_root);
                if physical_overlap || logical_overlap {
                    return Err(PathMappingError::AmbiguousRules(
                        left.logical_root.clone(),
                        right.logical_root.clone(),
                    ));
                }
            }
        }
        rules.sort_by(|left, right| {
            right
                .physical_root
                .components()
                .count()
                .cmp(&left.physical_root.components().count())
                .then_with(|| left.logical_root.cmp(&right.logical_root))
        });

        let supplied = generated_path.into();
        let generated_path = normalize_logical_path(&supplied)
            .map_err(|_| PathMappingError::InvalidLogicalPath(supplied.clone()))?;
        if generated_path != supplied {
            return Err(PathMappingError::InvalidLogicalPath(supplied));
        }

        let mut canonical = String::from("follang.parc.path-mapping.v1\n");
        for logical_root in logical_roots {
            canonical.push_str(&logical_root);
            canonical.push('\n');
        }
        canonical.push_str(&generated_path);
        let fingerprint = ContentFingerprint::from_content(canonical.as_bytes());
        Ok(Self {
            rules,
            generated_path,
            fingerprint,
        })
    }

    pub fn map_path(&self, path: impl AsRef<Path>) -> Result<String, PathMappingError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(PathMappingError::InvalidPhysicalRoot(
                path.display().to_string(),
            ));
        }
        let absolute =
            std::fs::canonicalize(path).map_err(|error| PathMappingError::Canonicalize {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        if absolute
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(PathMappingError::InvalidPhysicalRoot(
                absolute.display().to_string(),
            ));
        }
        for rule in &self.rules {
            if let Ok(relative) = absolute.strip_prefix(&rule.physical_root) {
                let relative = relative
                    .to_str()
                    .ok_or_else(|| PathMappingError::NonUtf8Path(relative.display().to_string()))?;
                let relative = relative.replace('\\', "/");
                let joined = if relative.is_empty() {
                    rule.logical_root.clone()
                } else {
                    format!("{}/{}", rule.logical_root, relative)
                };
                return normalize_logical_path(&joined)
                    .map_err(|_| PathMappingError::InvalidLogicalPath(joined));
            }
        }
        Err(PathMappingError::UnmappedPath(
            absolute.display().to_string(),
        ))
    }

    pub fn generated_path(&self) -> &str {
        &self.generated_path
    }

    pub const fn fingerprint(&self) -> ContentFingerprint {
        self.fingerprint
    }
}

fn logical_root_contains(parent: &str, child: &str) -> bool {
    child == parent
        || child
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    Hermetic,
    Captured { variables: Vec<String> },
}

impl EnvironmentPolicy {
    pub fn captured(
        variables: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, ScanConfigError> {
        let mut variables: Vec<_> = variables.into_iter().map(Into::into).collect();
        if variables.iter().any(|name| !valid_environment_name(name)) {
            return Err(ScanConfigError::InvalidEnvironmentVariable);
        }
        variables.sort();
        variables.dedup();
        Ok(Self::Captured { variables })
    }
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_uppercase() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreprocessorMode {
    Builtin,
    External { executable: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScanConfigError {
    #[error("captured environment variable names must be uppercase ASCII identifiers")]
    InvalidEnvironmentVariable,
    #[error("PARC currently supports only C11 and C17 scanning")]
    UnsupportedLanguageStandard,
    #[error("PARC currently supports strict, GNU, or Clang C extension profiles")]
    UnsupportedExtensionFamily,
    #[error("invalid preprocessor macro name: {0}")]
    InvalidDefineName(String),
    #[error("macro {name} has a value containing a NUL or line break")]
    InvalidDefineValue { name: String },
    #[error("{role} path must be an absolute {expected}: {path}")]
    InvalidOperationalPath {
        role: &'static str,
        expected: &'static str,
        path: String,
    },
    #[error("an operational sysroot is valid only for an external preprocessor whose target declares a sysroot")]
    InvalidOperationalSysroot,
    #[error("scanning accepts exactly one entry header; multiple translation units must be scanned independently")]
    MultipleEntryHeaders,
    #[error("unsupported target visibility argument: {0}")]
    UnsupportedVisibilityArgument(String),
    #[error("mapped source path collides with the reserved generated path: {0}")]
    GeneratedPathCollision(String),
    #[error(
        "every scan resource limit must be nonzero and no greater than the production ceiling"
    )]
    InvalidLimits,
    #[error(transparent)]
    PathMapping(#[from] PathMappingError),
}

/// Configuration for a canonical source scan. There is intentionally no
/// default or host-target constructor.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub(crate) target: TargetSpec,
    pub(crate) path_mapping: PathMapping,
    pub(crate) preprocessor: PreprocessorMode,
    pub(crate) environment: EnvironmentPolicy,
    pub(crate) entry_headers: Vec<PathBuf>,
    pub(crate) forced_includes: Vec<PathBuf>,
    pub(crate) include_dirs: Vec<PathBuf>,
    pub(crate) system_include_dirs: Vec<PathBuf>,
    pub(crate) define_events: Vec<DefineEvent>,
    pub(crate) external_sysroot: Option<PathBuf>,
    pub(crate) limits: ScanLimits,
}

impl ScanConfig {
    pub fn new(
        target: TargetSpec,
        path_mapping: PathMapping,
        preprocessor: PreprocessorMode,
    ) -> Result<Self, ScanConfigError> {
        if !matches!(
            target.language_standard(),
            LanguageStandard::C11 | LanguageStandard::C17
        ) {
            return Err(ScanConfigError::UnsupportedLanguageStandard);
        }
        if !matches!(
            target.extension_profile().family,
            ExtensionFamily::Strict | ExtensionFamily::Gnu | ExtensionFamily::Clang
        ) {
            return Err(ScanConfigError::UnsupportedExtensionFamily);
        }
        Ok(Self {
            target,
            path_mapping,
            preprocessor,
            environment: EnvironmentPolicy::Hermetic,
            entry_headers: Vec::new(),
            forced_includes: Vec::new(),
            include_dirs: Vec::new(),
            system_include_dirs: Vec::new(),
            define_events: Vec::new(),
            external_sysroot: None,
            limits: ScanLimits::production(),
        })
    }

    pub fn entry_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.entry_headers.push(path.into());
        self
    }

    pub fn forced_include(mut self, path: impl Into<PathBuf>) -> Self {
        self.forced_includes.push(path.into());
        self
    }

    pub fn include_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.include_dirs.push(path.into());
        self
    }

    pub fn system_include_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.system_include_dirs.push(path.into());
        self
    }

    pub fn define(mut self, name: impl Into<String>, value: Option<String>) -> Self {
        self.define_events.push(DefineEvent::Define {
            name: name.into(),
            value,
        });
        self
    }

    pub fn undefine(mut self, name: impl Into<String>) -> Self {
        self.define_events
            .push(DefineEvent::Undefine { name: name.into() });
        self
    }

    pub fn with_environment(mut self, policy: EnvironmentPolicy) -> Self {
        self.environment = policy;
        self
    }

    pub fn with_external_sysroot(mut self, path: impl Into<PathBuf>) -> Self {
        self.external_sysroot = Some(path.into());
        self
    }

    pub fn with_limits(mut self, limits: ScanLimits) -> Result<Self, ScanConfigError> {
        if !limits.is_valid() {
            return Err(ScanConfigError::InvalidLimits);
        }
        self.limits = limits;
        Ok(self)
    }

    pub fn limits(&self) -> &ScanLimits {
        &self.limits
    }

    pub fn target(&self) -> &TargetSpec {
        &self.target
    }

    pub fn path_mapping(&self) -> &PathMapping {
        &self.path_mapping
    }

    pub fn parser_flavor(&self) -> Flavor {
        match self.target.extension_profile().family {
            ExtensionFamily::Strict => Flavor::StdC11,
            ExtensionFamily::Gnu => Flavor::GnuC11,
            ExtensionFamily::Clang => Flavor::ClangC11,
            ExtensionFamily::Msvc => unreachable!("MSVC rejected by ScanConfig::new"),
        }
    }

    /// Re-check every operational fact immediately before a scan.
    ///
    /// Paths are deliberately not normalized relative to the process current
    /// directory: all source and include arguments must be explicit absolute
    /// paths covered by [`PathMapping`].
    pub fn validate(&self) -> Result<(), ScanConfigError> {
        if !self.limits.is_valid() {
            return Err(ScanConfigError::InvalidLimits);
        }
        if self.entry_headers.len() > 1 {
            return Err(ScanConfigError::MultipleEntryHeaders);
        }
        for argument in self.target.abi_flags() {
            if let Some(value) = argument.as_str().strip_prefix("-fvisibility=") {
                if !matches!(value, "default" | "hidden" | "protected" | "internal") {
                    return Err(ScanConfigError::UnsupportedVisibilityArgument(
                        argument.as_str().to_owned(),
                    ));
                }
            }
        }
        for event in &self.define_events {
            match event {
                DefineEvent::Define { name, value } => {
                    validate_macro_name(name)?;
                    if value.as_ref().is_some_and(|value| {
                        value
                            .bytes()
                            .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
                    }) {
                        return Err(ScanConfigError::InvalidDefineValue { name: name.clone() });
                    }
                }
                DefineEvent::Undefine { name } => validate_macro_name(name)?,
            }
        }

        for path in &self.entry_headers {
            validate_mapped_path(&self.path_mapping, "entry header", "file", path, false)?;
        }
        for path in &self.forced_includes {
            validate_mapped_path(&self.path_mapping, "forced include", "file", path, false)?;
        }
        for path in &self.include_dirs {
            validate_mapped_path(
                &self.path_mapping,
                "include search",
                "directory",
                path,
                true,
            )?;
        }
        for path in &self.system_include_dirs {
            validate_mapped_path(
                &self.path_mapping,
                "system include search",
                "directory",
                path,
                true,
            )?;
        }

        match (
            &self.preprocessor,
            self.target.sysroot(),
            &self.external_sysroot,
        ) {
            (PreprocessorMode::Builtin, _, Some(_))
            | (PreprocessorMode::External { .. }, None, Some(_))
            | (PreprocessorMode::External { .. }, Some(_), None) => {
                return Err(ScanConfigError::InvalidOperationalSysroot)
            }
            _ => {}
        }
        if let PreprocessorMode::External { executable } = &self.preprocessor {
            validate_absolute_path("preprocessor executable", "file", executable, false)?;
        }
        if let Some(sysroot) = &self.external_sysroot {
            validate_absolute_path("operational sysroot", "directory", sysroot, true)?;
        }
        Ok(())
    }
}

fn validate_macro_name(name: &str) -> Result<(), ScanConfigError> {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return Err(ScanConfigError::InvalidDefineName(name.to_owned()));
    };
    if !(first == b'_' || first.is_ascii_alphabetic())
        || !bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
    {
        return Err(ScanConfigError::InvalidDefineName(name.to_owned()));
    }
    Ok(())
}

fn validate_mapped_path(
    mapping: &PathMapping,
    role: &'static str,
    expected: &'static str,
    path: &Path,
    directory: bool,
) -> Result<(), ScanConfigError> {
    validate_absolute_path(role, expected, path, directory)?;
    let logical = mapping.map_path(path)?;
    if logical == mapping.generated_path() {
        return Err(ScanConfigError::GeneratedPathCollision(logical));
    }
    Ok(())
}

fn validate_absolute_path(
    role: &'static str,
    expected: &'static str,
    path: &Path,
    directory: bool,
) -> Result<(), ScanConfigError> {
    let valid = path.is_absolute()
        && if directory {
            path.is_dir()
        } else {
            path.is_file()
        };
    if !valid {
        return Err(ScanConfigError::InvalidOperationalPath {
            role,
            expected,
            path: path.display().to_string(),
        });
    }
    Ok(())
}
