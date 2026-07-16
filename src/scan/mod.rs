//! Explicit-target scan pipeline producing the checked schema-v2 contract.

pub mod config;
mod traced;

pub use config::{
    EnvironmentPolicy, PathMapping, PathMappingError, PathMappingRule, PreprocessorMode,
    ScanConfig, ScanConfigError, ScanLimits,
};

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::contract::*;
use crate::extract::{extract_contract, ExtractionContext};

const GENERATED_PROVENANCE_CODE: &str = "PARC-P0001";
const RECOVERY_CODE: &str = "PARC-P0002";
const BUILTIN_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("no entry headers were configured")]
    NoEntryHeaders,
    #[error(transparent)]
    Configuration(#[from] ScanConfigError),
    #[error(transparent)]
    PathMapping(#[from] PathMappingError),
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("preprocessor executable must be an absolute file path: {0}")]
    InvalidExecutable(String),
    #[error("external target has a sysroot identity but no operational sysroot path")]
    MissingOperationalSysroot,
    #[error("operational sysroot was supplied for a target with no sysroot identity")]
    UnexpectedOperationalSysroot,
    #[error("external preprocessor failed: {0}")]
    ExternalPreprocessor(String),
    #[error("built-in preprocessor failed: {0}")]
    BuiltinPreprocessor(String),
    #[error("preprocessor output is not UTF-8: {0}")]
    NonUtf8Output(String),
    #[error("the built-in preprocessor does not support target {0}")]
    UnsupportedBuiltinTarget(String),
    #[error("source artifact exceeds the contract's 64-bit byte range")]
    SizeOverflow,
    #[error("path is not valid UTF-8 and cannot be recorded canonically: {0}")]
    NonUtf8Path(String),
    #[error("captured environment variable {0} is not valid UTF-8")]
    NonUtf8Environment(String),
    #[error("preprocessor executable content differs from TargetSpec.compiler")]
    CompilerExecutableMismatch,
    #[error("{code}: {message}")]
    ResourceLimit {
        code: &'static str,
        message: &'static str,
    },
    #[error("external preprocessing requires safe process-group termination on this host")]
    UnsupportedExternalHost,
    #[error(transparent)]
    Contract(#[from] SourcePackageBuildError),
}

struct MaterializedFile {
    contract: SourceFile,
}

struct EnvironmentCapture {
    contract: EnvironmentInputs,
    values: Vec<(String, String)>,
    include_paths: Vec<(PathBuf, IncludeSearchKind)>,
}

struct Preprocessed {
    text: String,
    identity: PreprocessorIdentity,
    warnings: Vec<String>,
    issues: Vec<PreprocessIssue>,
}

struct PreprocessIssue {
    code: &'static str,
    severity: Severity,
    impact: DiagnosticCompletenessImpact,
    message: String,
}

/// Scan configured headers into a checked schema-v2 package.
///
/// The certified built-in path can return `Complete` when every range and
/// preprocessing decision carries exact original provenance. External output
/// remains explicitly partial because its original macro/include provenance
/// cannot be proven from generated text alone.
pub fn scan_headers(config: &ScanConfig) -> Result<ScanReport, ScanError> {
    config.validate()?;
    if config.entry_headers.is_empty() {
        return Err(ScanError::NoEntryHeaders);
    }

    let environment = capture_environment(&config.environment)?;
    let mut file_table = BTreeMap::new();
    let mut entry_files = Vec::new();
    for path in &config.entry_headers {
        entry_files.push(mapped_file_id(config, path)?);
    }
    let mut forced_includes = Vec::new();
    for path in &config.forced_includes {
        forced_includes.push(mapped_file_id(config, path)?);
    }
    if matches!(config.preprocessor, PreprocessorMode::External { .. }) {
        let mut total_input_bytes = 0_u64;
        for path in &config.entry_headers {
            let materialized =
                materialize_file(config, path, SourceFileRole::Entry, &mut total_input_bytes)?;
            file_table.insert(materialized.contract.id, materialized.contract);
        }
        for path in &config.forced_includes {
            let materialized = materialize_file(
                config,
                path,
                SourceFileRole::UserInclude,
                &mut total_input_bytes,
            )?;
            file_table
                .entry(materialized.contract.id)
                .or_insert(materialized.contract);
        }
    }

    let mut include_search = Vec::new();
    for path in &config.include_dirs {
        include_search.push(include_search_entry(config, path, IncludeSearchKind::User)?);
    }
    for path in &config.system_include_dirs {
        include_search.push(include_search_entry(
            config,
            path,
            IncludeSearchKind::System,
        )?);
    }
    for (path, kind) in &environment.include_paths {
        include_search.push(include_search_entry(config, path, *kind)?);
    }

    let mut traced = None;
    let preprocessed = match &config.preprocessor {
        PreprocessorMode::Builtin => {
            let value = traced::preprocess_builtin_traced(config, &environment)?;
            let text = value.text.clone();
            traced = Some(value);
            Preprocessed {
                text,
                identity: PreprocessorIdentity::Builtin {
                    implementation_version: BUILTIN_VERSION.to_owned(),
                },
                warnings: Vec::new(),
                issues: Vec::new(),
            }
        }
        PreprocessorMode::External { executable } => {
            preprocess_external(config, executable, &environment)?
        }
    };

    let generated_path = config.path_mapping.generated_path().to_owned();
    let generated_id = FileId::from_logical_path(&generated_path)
        .expect("PathMapping validates generated logical path");
    let generated_file = source_file(
        generated_id,
        generated_path,
        SourceFileRole::Generated,
        preprocessed.text.as_bytes(),
    )?;
    file_table.insert(generated_id, generated_file);

    let recovered =
        crate::parse::translation_unit_resilient(&preprocessed.text, config.parser_flavor());
    let mut extracted = extract_contract(
        &recovered.unit,
        ExtractionContext {
            source: &preprocessed.text,
            generated_file: generated_id,
            target: config.target.fingerprint(),
            int128_supported: config.target.c_data_model().int128_layout.is_some(),
            default_visibility: target_default_visibility(&config.target),
        },
    );

    let generated_range = SourceRange {
        file: generated_id,
        start: 0,
        end: u64::try_from(preprocessed.text.len()).map_err(|_| ScanError::SizeOverflow)?,
    };
    let mut diagnostics = extracted.diagnostics;
    let mut macros = Vec::new();
    if let Some(trace) = &traced {
        let remap_issues = remap_extraction(
            &mut extracted.declarations,
            &mut diagnostics,
            trace,
            generated_id,
        );
        for file in trace.files.values() {
            file_table.insert(file.id, file.clone());
        }
        macros = trace.macros.clone();
        for issue in &trace.issues {
            diagnostics.push(SourceDiagnostic {
                code: diagnostic_code(issue.code),
                stage: DiagnosticStage::Preprocess,
                severity: issue.severity,
                completeness_impact: issue.impact,
                message: issue.message.clone(),
                range: issue.range,
                related: Vec::new(),
                declaration: None,
                target: config.target.fingerprint(),
            });
        }
        for issue in remap_issues {
            diagnostics.push(SourceDiagnostic {
                code: diagnostic_code(issue.code),
                stage: DiagnosticStage::Preprocess,
                severity: issue.severity,
                completeness_impact: issue.impact,
                message: issue.message,
                range: issue.range,
                related: Vec::new(),
                declaration: None,
                target: config.target.fingerprint(),
            });
        }
        for entry in &mut include_search {
            entry.content = Some(
                trace
                    .used_search_content
                    .get(&entry.logical_path)
                    .copied()
                    .unwrap_or_else(|| {
                        let mut bytes = b"follang.parc.effective-include-search.v1\0".to_vec();
                        bytes.extend_from_slice(entry.logical_path.as_bytes());
                        ContentFingerprint::from_content(&bytes)
                    }),
            );
        }
    } else {
        let generated_reason = CompletenessReason {
            code: diagnostic_code(GENERATED_PROVENANCE_CODE),
            message: "external preprocessing produced generated source without exact original include and macro provenance".to_owned(),
            range: Some(generated_range),
        };
        diagnostics.push(SourceDiagnostic {
            code: generated_reason.code.clone(),
            stage: DiagnosticStage::Preprocess,
            severity: Severity::Warning,
            completeness_impact: DiagnosticCompletenessImpact::ForcesPartial,
            message: generated_reason.message.clone(),
            range: generated_reason.range,
            related: Vec::new(),
            declaration: None,
            target: config.target.fingerprint(),
        });
    }
    for warning in preprocessed.warnings {
        diagnostics.push(SourceDiagnostic {
            code: diagnostic_code("PARC-P0003"),
            stage: DiagnosticStage::Preprocess,
            severity: Severity::Warning,
            completeness_impact: DiagnosticCompletenessImpact::ForcesPartial,
            message: warning,
            range: Some(generated_range),
            related: Vec::new(),
            declaration: None,
            target: config.target.fingerprint(),
        });
    }
    for issue in preprocessed.issues {
        diagnostics.push(SourceDiagnostic {
            code: diagnostic_code(issue.code),
            stage: DiagnosticStage::Preprocess,
            severity: issue.severity,
            completeness_impact: issue.impact,
            message: issue.message,
            range: None,
            related: Vec::new(),
            declaration: None,
            target: config.target.fingerprint(),
        });
    }
    for recovery in recovered.errors {
        let range = traced
            .as_ref()
            .and_then(|trace| trace.map_span(recovery.skipped).map(|mapped| mapped.0))
            .or_else(|| {
                generated_span_range(generated_id, recovery.skipped, preprocessed.text.len())
            });
        let mut expected = recovery.error.expected.into_iter().collect::<Vec<_>>();
        expected.sort_unstable();
        let message = format!(
            "parser recovery skipped bytes after error at {}:{}: {expected:?}",
            recovery.error.line, recovery.error.column
        );
        diagnostics.push(SourceDiagnostic {
            code: diagnostic_code(RECOVERY_CODE),
            stage: DiagnosticStage::Recovery,
            severity: Severity::Error,
            completeness_impact: DiagnosticCompletenessImpact::ForcesPartial,
            message,
            range,
            related: Vec::new(),
            declaration: None,
            target: config.target.fingerprint(),
        });
    }
    diagnostics.sort();
    diagnostics.dedup();

    let completeness = completeness_from_diagnostics(&diagnostics);
    let input = SourcePackageInput {
        target: config.target.clone(),
        files: file_table.into_values().collect(),
        inputs: EffectiveSourceInputs {
            entry_files,
            include_search,
            define_events: config.define_events.clone(),
            forced_includes,
            preprocessor: preprocessed.identity,
            environment: environment.contract,
            path_mapping_fingerprint: config.path_mapping.fingerprint(),
        },
        declarations: extracted.declarations,
        macros,
        diagnostics,
        completeness,
    };

    // The generated translation-unit buffer is an extraction implementation
    // detail for traced built-in scans. Drop it only when the checked contract
    // proves that every range and provenance edge was remapped to an original
    // file. Keeping an unreferenced fixed generated FileId would make two
    // otherwise compatible translation units impossible to merge when their
    // preprocessed bytes differ.
    if traced.is_some() {
        let mut original_only = input.clone();
        original_only.files.retain(|file| file.id != generated_id);
        if let Ok(package) = SourcePackage::try_new(original_only) {
            return Ok(ScanReport::new(package));
        }
    }
    Ok(ScanReport::new(SourcePackage::try_new(input)?))
}

fn remap_extraction(
    declarations: &mut [SourceDeclaration],
    diagnostics: &mut [SourceDiagnostic],
    trace: &traced::TracedPreprocessed,
    generated_file: FileId,
) -> Vec<traced::TraceIssue> {
    let mut issues = Vec::new();
    for declaration in declarations {
        for occurrence in &mut declaration.occurrences {
            if occurrence.range.file != generated_file {
                continue;
            }
            let generated_range = occurrence.range;
            if let Some((range, provenance)) = map_contract_range(trace, generated_range) {
                occurrence.range = range;
                occurrence.provenance = provenance;
                if let Some(spelling) = trace.source_text(range) {
                    occurrence.spelling = spelling.to_owned();
                }
                if let Some(name_range) = occurrence.name_range {
                    if let Some((mapped, _)) = map_contract_range(trace, name_range) {
                        occurrence.name_range = Some(mapped);
                    } else {
                        occurrence.name_range = None;
                        issues.push(provenance_gap(name_range, "declaration name range"));
                    }
                }
                remap_attributes(
                    &mut occurrence.attributes,
                    trace,
                    generated_file,
                    &mut issues,
                );
                occurrence.id = OccurrenceId::derive(
                    declaration.id,
                    range.file,
                    &canonical_tokens_bytes(&occurrence.normalized_tokens),
                    occurrence.duplicate_ordinal,
                );
            } else {
                issues.push(provenance_gap(generated_range, "declaration occurrence"));
            }
        }
        declaration
            .occurrences
            .sort_by_key(|occurrence| occurrence.id);
        match &mut declaration.kind {
            SourceDeclarationKind::Function(function) => {
                for parameter in &mut function.parameters {
                    remap_child(
                        &mut parameter.range,
                        &mut parameter.provenance,
                        &mut parameter.attributes,
                        trace,
                        generated_file,
                        &mut issues,
                    );
                }
            }
            SourceDeclarationKind::Record(record) => {
                for field in &mut record.fields {
                    remap_child(
                        &mut field.range,
                        &mut field.provenance,
                        &mut field.attributes,
                        trace,
                        generated_file,
                        &mut issues,
                    );
                }
            }
            SourceDeclarationKind::Enum(enumeration) => {
                for variant in &mut enumeration.variants {
                    remap_child(
                        &mut variant.range,
                        &mut variant.provenance,
                        &mut variant.attributes,
                        trace,
                        generated_file,
                        &mut issues,
                    );
                }
            }
            SourceDeclarationKind::TypeAlias(_)
            | SourceDeclarationKind::Variable(_)
            | SourceDeclarationKind::Unsupported(_) => {}
        }
    }
    for diagnostic in diagnostics {
        if let Some(range) = diagnostic.range {
            if range.file == generated_file {
                if let Some((mapped, _)) = map_contract_range(trace, range) {
                    diagnostic.range = Some(mapped);
                } else {
                    issues.push(provenance_gap(range, "diagnostic range"));
                }
            }
        }
        for related in &mut diagnostic.related {
            if related.range.file == generated_file {
                if let Some((range, _)) = map_contract_range(trace, related.range) {
                    related.range = range;
                } else {
                    issues.push(provenance_gap(related.range, "related diagnostic range"));
                }
            }
        }
    }
    issues
}

fn remap_child(
    range: &mut SourceRange,
    provenance: &mut SourceProvenance,
    attributes: &mut [SourceAttribute],
    trace: &traced::TracedPreprocessed,
    generated_file: FileId,
    issues: &mut Vec<traced::TraceIssue>,
) {
    if range.file == generated_file {
        if let Some((mapped, mapped_provenance)) = map_contract_range(trace, *range) {
            *range = mapped;
            *provenance = mapped_provenance;
        } else {
            issues.push(provenance_gap(*range, "nested declaration range"));
        }
    }
    remap_attributes(attributes, trace, generated_file, issues);
}

fn remap_attributes(
    attributes: &mut [SourceAttribute],
    trace: &traced::TracedPreprocessed,
    generated_file: FileId,
    issues: &mut Vec<traced::TraceIssue>,
) {
    for attribute in attributes {
        if attribute.range.file != generated_file {
            continue;
        }
        if let Some((range, _)) = map_contract_range(trace, attribute.range) {
            attribute.range = range;
            if let Some(spelling) = trace.source_text(range) {
                attribute.spelling = spelling.to_owned();
            }
        } else {
            issues.push(provenance_gap(attribute.range, "source attribute range"));
        }
    }
}

fn provenance_gap(range: SourceRange, subject: &str) -> traced::TraceIssue {
    traced::TraceIssue {
        code: "PARC-P2000",
        severity: Severity::Warning,
        impact: DiagnosticCompletenessImpact::ForcesPartial,
        message: format!("{subject} could not be mapped to one exact original-source provenance"),
        range: Some(range),
    }
}

fn map_contract_range(
    trace: &traced::TracedPreprocessed,
    range: SourceRange,
) -> Option<(SourceRange, SourceProvenance)> {
    let start = usize::try_from(range.start).ok()?;
    let end = usize::try_from(range.end).ok()?;
    trace.map_generated_range(start, end)
}

fn materialize_file(
    config: &ScanConfig,
    path: &Path,
    role: SourceFileRole,
    total_input_bytes: &mut u64,
) -> Result<MaterializedFile, ScanError> {
    let canonical = std::fs::canonicalize(path).map_err(|source| ScanError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let remaining_total = config
        .limits
        .max_total_input_bytes
        .checked_sub(*total_input_bytes)
        .ok_or(ScanError::ResourceLimit {
            code: "PARC-E2202",
            message: "transitive source bytes exceeded max_total_input_bytes",
        })?;
    let metadata = std::fs::metadata(&canonical).map_err(|source| ScanError::Read {
        path: canonical.display().to_string(),
        source,
    })?;
    if metadata.len() > config.limits.max_input_file_bytes {
        return Err(ScanError::ResourceLimit {
            code: "PARC-E2201",
            message: "source file exceeded max_input_file_bytes",
        });
    }
    if metadata.len() > remaining_total {
        return Err(ScanError::ResourceLimit {
            code: "PARC-E2202",
            message: "transitive source bytes exceeded max_total_input_bytes",
        });
    }
    let cap = config.limits.max_input_file_bytes.min(remaining_total);
    let content = read_bounded_file(&canonical, cap)?.ok_or(ScanError::ResourceLimit {
        code: if config.limits.max_input_file_bytes <= remaining_total {
            "PARC-E2201"
        } else {
            "PARC-E2202"
        },
        message: if config.limits.max_input_file_bytes <= remaining_total {
            "source file exceeded max_input_file_bytes"
        } else {
            "transitive source bytes exceeded max_total_input_bytes"
        },
    })?;
    let content_len = u64::try_from(content.len()).map_err(|_| ScanError::SizeOverflow)?;
    *total_input_bytes = total_input_bytes
        .checked_add(content_len)
        .ok_or(ScanError::SizeOverflow)?;
    let logical_path = config.path_mapping.map_path(&canonical)?;
    let id = FileId::from_logical_path(&logical_path)
        .expect("PathMapping returns canonical logical paths");
    Ok(MaterializedFile {
        contract: source_file(id, logical_path, role, &content)?,
    })
}

fn mapped_file_id(config: &ScanConfig, path: &Path) -> Result<FileId, ScanError> {
    let logical_path = config.path_mapping.map_path(path)?;
    Ok(FileId::from_logical_path(&logical_path)
        .expect("PathMapping returns canonical logical paths"))
}

pub(super) fn read_bounded_file(path: &Path, cap: u64) -> Result<Option<Vec<u8>>, ScanError> {
    let file = std::fs::File::open(path).map_err(|source| ScanError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let mut content = Vec::new();
    file.take(cap.checked_add(1).ok_or(ScanError::SizeOverflow)?)
        .read_to_end(&mut content)
        .map_err(|source| ScanError::Read {
            path: path.display().to_string(),
            source,
        })?;
    if u64::try_from(content.len()).map_err(|_| ScanError::SizeOverflow)? > cap {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

fn source_file(
    id: FileId,
    logical_path: String,
    role: SourceFileRole,
    content: &[u8],
) -> Result<SourceFile, ScanError> {
    Ok(SourceFile {
        id,
        logical_path,
        role,
        content: ContentFingerprint::from_content(content),
        byte_len: u64::try_from(content.len()).map_err(|_| ScanError::SizeOverflow)?,
        line_starts: line_starts(content)?,
    })
}

fn line_starts(content: &[u8]) -> Result<Vec<u64>, ScanError> {
    let mut starts = vec![0];
    for (index, byte) in content.iter().enumerate() {
        if *byte == b'\n' {
            starts.push(u64::try_from(index + 1).map_err(|_| ScanError::SizeOverflow)?);
        }
    }
    Ok(starts)
}

fn include_search_entry(
    config: &ScanConfig,
    path: &Path,
    kind: IncludeSearchKind,
) -> Result<IncludeSearchEntry, ScanError> {
    Ok(IncludeSearchEntry {
        logical_path: config.path_mapping.map_path(path)?,
        kind,
        content: None,
    })
}

fn capture_environment(policy: &EnvironmentPolicy) -> Result<EnvironmentCapture, ScanError> {
    const DOMAIN: &[u8] = b"follang.parc.environment-value.v1\0";

    match policy {
        EnvironmentPolicy::Hermetic => Ok(EnvironmentCapture {
            contract: EnvironmentInputs::Hermetic,
            values: Vec::new(),
            include_paths: Vec::new(),
        }),
        EnvironmentPolicy::Captured { variables } => {
            let mut values = Vec::new();
            let mut captured = Vec::new();
            let mut include_paths = Vec::new();
            for name in variables {
                match std::env::var(name) {
                    Ok(value) => {
                        let mut fingerprint_input =
                            Vec::with_capacity(DOMAIN.len() + value.len() + 4);
                        fingerprint_input.extend_from_slice(DOMAIN);
                        fingerprint_input.extend_from_slice(b"set\0");
                        fingerprint_input.extend_from_slice(value.as_bytes());
                        captured.push(CapturedEnvironment {
                            name: name.clone(),
                            value_fingerprint: ContentFingerprint::from_content(&fingerprint_input),
                        });
                        if matches!(name.as_str(), "CPATH" | "C_INCLUDE_PATH") {
                            let kind = if name == "CPATH" {
                                IncludeSearchKind::User
                            } else {
                                IncludeSearchKind::System
                            };
                            include_paths
                                .extend(std::env::split_paths(&value).map(|path| (path, kind)));
                        }
                        values.push((name.clone(), value));
                    }
                    Err(std::env::VarError::NotPresent) => {
                        let mut fingerprint_input = Vec::with_capacity(DOMAIN.len() + 5);
                        fingerprint_input.extend_from_slice(DOMAIN);
                        fingerprint_input.extend_from_slice(b"unset");
                        captured.push(CapturedEnvironment {
                            name: name.clone(),
                            value_fingerprint: ContentFingerprint::from_content(&fingerprint_input),
                        });
                    }
                    Err(std::env::VarError::NotUnicode(_)) => {
                        return Err(ScanError::NonUtf8Environment(name.clone()))
                    }
                }
            }
            Ok(EnvironmentCapture {
                contract: EnvironmentInputs::Captured {
                    variables: captured,
                },
                values,
                include_paths,
            })
        }
    }
}

fn preprocess_external(
    config: &ScanConfig,
    executable: &Path,
    environment: &EnvironmentCapture,
) -> Result<Preprocessed, ScanError> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(ScanError::InvalidExecutable(
            executable.display().to_string(),
        ));
    }
    match (config.target.sysroot(), &config.external_sysroot) {
        (Some(_), None) => return Err(ScanError::MissingOperationalSysroot),
        (None, Some(_)) => return Err(ScanError::UnexpectedOperationalSysroot),
        _ => {}
    }

    let executable_file = std::fs::File::open(executable).map_err(|source| ScanError::Read {
        path: executable.display().to_string(),
        source,
    })?;
    let executable_len = executable_file
        .metadata()
        .map_err(|source| ScanError::Read {
            path: executable.display().to_string(),
            source,
        })?
        .len();
    let executable_fingerprint = ContentFingerprint::from_reader(executable_file, executable_len)
        .map_err(|source| ScanError::Read {
        path: executable.display().to_string(),
        source,
    })?;
    if executable_fingerprint != config.target.compiler().executable_content() {
        return Err(ScanError::CompilerExecutableMismatch);
    }

    let mut command_arguments = Vec::<OsString>::new();
    let mut arguments = Vec::<String>::new();
    push_argument(&mut command_arguments, &mut arguments, "-E");
    push_argument(&mut command_arguments, &mut arguments, "-P");
    let standard = match (
        config.target.language_standard(),
        config.target.extension_profile().family,
    ) {
        (LanguageStandard::C11, ExtensionFamily::Strict) => "-std=c11",
        (LanguageStandard::C11, ExtensionFamily::Gnu | ExtensionFamily::Clang) => "-std=gnu11",
        (LanguageStandard::C17, ExtensionFamily::Strict) => "-std=c17",
        (LanguageStandard::C17, ExtensionFamily::Gnu | ExtensionFamily::Clang) => "-std=gnu17",
        _ => unreachable!("ScanConfig rejects unsupported target dialects"),
    };
    push_argument(&mut command_arguments, &mut arguments, standard);
    if matches!(
        config.target.compiler().family(),
        CompilerFamily::Clang | CompilerFamily::AppleClang
    ) {
        push_argument(
            &mut command_arguments,
            &mut arguments,
            format!("--target={}", config.target.triple()),
        );
    }
    if let Some(sysroot) = &config.external_sysroot {
        push_argument(&mut command_arguments, &mut arguments, "--sysroot");
        command_arguments.push(sysroot.as_os_str().to_owned());
        arguments.push(
            config
                .target
                .sysroot()
                .expect("presence checked")
                .logical_path()
                .to_owned(),
        );
    }
    for argument in config.target.abi_flags() {
        push_argument(&mut command_arguments, &mut arguments, argument.as_str());
    }
    for directory in &config.include_dirs {
        push_argument(&mut command_arguments, &mut arguments, "-I");
        push_mapped_path(config, &mut command_arguments, &mut arguments, directory)?;
    }
    for directory in &config.system_include_dirs {
        push_argument(&mut command_arguments, &mut arguments, "-isystem");
        push_mapped_path(config, &mut command_arguments, &mut arguments, directory)?;
    }
    for path in &config.forced_includes {
        push_argument(&mut command_arguments, &mut arguments, "-include");
        push_mapped_path(config, &mut command_arguments, &mut arguments, path)?;
    }
    for event in &config.define_events {
        match event {
            DefineEvent::Define { name, value } => push_argument(
                &mut command_arguments,
                &mut arguments,
                match value {
                    Some(value) => format!("-D{name}={value}"),
                    None => format!("-D{name}"),
                },
            ),
            DefineEvent::Undefine { name } => {
                push_argument(&mut command_arguments, &mut arguments, format!("-U{name}"))
            }
        }
    }
    for path in &config.entry_headers {
        push_mapped_path(config, &mut command_arguments, &mut arguments, path)?;
    }

    let mut command = Command::new(executable);
    let working_directory = config.entry_headers[0]
        .parent()
        .expect("validated absolute entry header has a parent");
    command
        .args(&command_arguments)
        .current_dir(working_directory)
        .env_clear()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        return Err(ScanError::UnsupportedExternalHost);
    }
    for (name, value) in &environment.values {
        command.env(name, value);
    }
    let mut child = command
        .spawn()
        .map_err(|error| ScanError::ExternalPreprocessor(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout was configured as a pipe");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was configured as a pipe");
    let output_bytes = Arc::new(AtomicU64::new(0));
    let stdout_bytes = Arc::clone(&output_bytes);
    let stderr_bytes = Arc::clone(&output_bytes);
    let output_limit = config.limits.max_external_output_bytes;
    let (limit_tx, limit_rx) = mpsc::channel();
    let stdout_limit_tx = limit_tx.clone();
    let stdout_reader = thread::spawn(move || {
        read_external_stream(stdout, stdout_bytes, output_limit, stdout_limit_tx)
    });
    let stderr_reader =
        thread::spawn(move || read_external_stream(stderr, stderr_bytes, output_limit, limit_tx));

    let started = Instant::now();
    let mut timed_out = false;
    let mut output_exceeded = false;
    let status = loop {
        if limit_rx.try_recv().is_ok() {
            output_exceeded = true;
            terminate_external_group(&mut child);
            break reap_external_child(&mut child, Duration::from_millis(50))?;
        }
        match child
            .try_wait()
            .map_err(|error| ScanError::ExternalPreprocessor(error.to_string()))?
        {
            Some(status) => break Some(status),
            None if started.elapsed() >= config.limits.external_timeout => {
                timed_out = true;
                terminate_external_group(&mut child);
                break reap_external_child(&mut child, Duration::from_millis(50))?;
            }
            None => thread::sleep(Duration::from_millis(2)),
        }
    };

    // A compiler must close both streams when it exits. A descendant that
    // inherited a pipe cannot make the producer wait forever: terminate the
    // dedicated process group and detach any reader that still fails to close.
    let mut cleanup_deadline = Instant::now() + Duration::from_millis(50);
    while (!stdout_reader.is_finished() || !stderr_reader.is_finished())
        && Instant::now() < cleanup_deadline
    {
        thread::sleep(Duration::from_millis(1));
    }
    let mut stream_cleanup_forced = false;
    if !stdout_reader.is_finished() || !stderr_reader.is_finished() {
        stream_cleanup_forced = true;
        terminate_external_group(&mut child);
        cleanup_deadline = Instant::now() + Duration::from_millis(50);
        while (!stdout_reader.is_finished() || !stderr_reader.is_finished())
            && Instant::now() < cleanup_deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
    }
    let readers_closed = stdout_reader.is_finished() && stderr_reader.is_finished();
    let stdout_result = readers_closed.then(|| stdout_reader.join());
    let stderr_result = readers_closed.then(|| stderr_reader.join());
    let (stdout, stdout_reader_exceeded, stdout_reader_failed) =
        unpack_external_reader(stdout_result);
    let (stderr, stderr_reader_exceeded, stderr_reader_failed) =
        unpack_external_reader(stderr_result);
    output_exceeded |= stdout_reader_exceeded
        || stderr_reader_exceeded
        || output_bytes.load(Ordering::SeqCst) > config.limits.max_external_output_bytes;
    let mut issues = Vec::new();
    if timed_out {
        issues.push(PreprocessIssue {
            code: "PARC-E2210",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: "external preprocessor exceeded external_timeout and was terminated"
                .to_owned(),
        });
    }
    if output_exceeded {
        issues.push(PreprocessIssue {
            code: "PARC-E2211",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message:
                "external preprocessor exceeded max_external_output_bytes; output was discarded"
                    .to_owned(),
        });
    }
    if stream_cleanup_forced || !readers_closed || stdout_reader_failed || stderr_reader_failed {
        issues.push(PreprocessIssue {
            code: "PARC-E2212",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: "external preprocessor streams did not close safely; output was discarded"
                .to_owned(),
        });
    }
    if status.is_none() {
        issues.push(PreprocessIssue {
            code: "PARC-E2212",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: "external preprocessor could not be reaped within the cleanup deadline"
                .to_owned(),
        });
    } else if status.is_some_and(|status| !status.success()) && !timed_out && !output_exceeded {
        issues.push(PreprocessIssue {
            code: "PARC-E2213",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: "external preprocessor exited unsuccessfully; output was discarded".to_owned(),
        });
    }
    let (text, stderr_present) = if issues.is_empty() {
        match (String::from_utf8(stdout), String::from_utf8(stderr)) {
            (Ok(stdout), Ok(stderr)) => (stdout, !stderr.is_empty()),
            _ => {
                issues.push(PreprocessIssue {
                    code: "PARC-E2214",
                    severity: Severity::Error,
                    impact: DiagnosticCompletenessImpact::ForcesRejected,
                    message:
                        "external preprocessor produced non-UTF-8 output; output was discarded"
                            .to_owned(),
                });
                (String::new(), false)
            }
        }
    } else {
        (String::new(), false)
    };
    Ok(Preprocessed {
        text,
        identity: PreprocessorIdentity::External {
            executable: config.target.compiler().logical_executable().to_owned(),
            executable_fingerprint,
            arguments,
        },
        warnings: stderr_present
            .then(|| "external preprocessor emitted stderr diagnostics".to_owned())
            .into_iter()
            .collect(),
        issues,
    })
}

type ExternalReaderResult = std::io::Result<(Vec<u8>, bool)>;

fn read_external_stream(
    mut reader: impl Read,
    total: Arc<AtomicU64>,
    limit: u64,
    limit_tx: Sender<()>,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::new();
    let mut exceeded = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let read = u64::try_from(read).expect("fixed buffer length fits u64");
        let previous = total.fetch_add(read, Ordering::SeqCst);
        let remaining = limit.saturating_sub(previous);
        let retained = read.min(remaining);
        output.extend_from_slice(
            &buffer[..usize::try_from(retained).expect("retained bytes fit fixed buffer")],
        );
        if retained < read || previous >= limit {
            exceeded = true;
            let _ = limit_tx.send(());
            break;
        }
    }
    Ok((output, exceeded))
}

fn unpack_external_reader(
    joined: Option<thread::Result<ExternalReaderResult>>,
) -> (Vec<u8>, bool, bool) {
    match joined {
        Some(Ok(Ok((bytes, exceeded)))) => (bytes, exceeded, false),
        Some(Ok(Err(_))) | Some(Err(_)) | None => (Vec::new(), false, true),
    }
}

fn terminate_external_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", group.as_str()])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    let _ = child.kill();
}

fn reap_external_child(
    child: &mut Child,
    timeout: Duration,
) -> Result<Option<std::process::ExitStatus>, ScanError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ScanError::ExternalPreprocessor(error.to_string()))?
        {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn push_argument(
    command_arguments: &mut Vec<OsString>,
    recorded_arguments: &mut Vec<String>,
    value: impl Into<String>,
) {
    let value = value.into();
    command_arguments.push(OsString::from(&value));
    recorded_arguments.push(value);
}

fn push_mapped_path(
    config: &ScanConfig,
    command_arguments: &mut Vec<OsString>,
    recorded_arguments: &mut Vec<String>,
    path: &Path,
) -> Result<(), ScanError> {
    command_arguments.push(path.as_os_str().to_owned());
    recorded_arguments.push(config.path_mapping.map_path(path)?);
    Ok(())
}

fn define_builtin_target_macros(
    table: &mut crate::preprocess::MacroTable,
    target: &TargetSpec,
) -> Result<(), ScanError> {
    use crate::preprocess::{MacroDef, Token, TokenKind};

    fn define(table: &mut crate::preprocess::MacroTable, name: &str, value: &str) {
        table.define(MacroDef {
            name: name.to_owned(),
            params: None,
            is_variadic: false,
            body: vec![Token {
                kind: TokenKind::Number,
                text: value.to_owned(),
                offset: 0,
            }],
        });
    }

    define(table, "__STDC__", "1");
    let stdc_version = match target.language_standard() {
        LanguageStandard::C11 => "201112L",
        LanguageStandard::C17 => "201710L",
        _ => unreachable!("ScanConfig rejects unsupported language standards"),
    };
    define(table, "__STDC_VERSION__", stdc_version);
    define(
        table,
        "__CHAR_BIT__",
        &target.c_data_model().char_bit.to_string(),
    );
    define(
        table,
        "__SIZEOF_POINTER__",
        &(target.c_data_model().pointer_layout.storage_bits / 8).to_string(),
    );
    define(
        table,
        "__SIZEOF_INT__",
        &(target.c_data_model().int_layout.storage_bits / 8).to_string(),
    );
    define(
        table,
        "__SIZEOF_LONG__",
        &(target.c_data_model().long_layout.storage_bits / 8).to_string(),
    );
    if target.c_data_model().char_signedness == CharSignedness::Unsigned {
        define(table, "__CHAR_UNSIGNED__", "1");
    }
    match target.architecture() {
        Architecture::X86_64 => define(table, "__x86_64__", "1"),
        Architecture::X86 => define(table, "__i386__", "1"),
        Architecture::Aarch64 => define(table, "__aarch64__", "1"),
        Architecture::Arm => define(table, "__arm__", "1"),
        architecture => {
            return Err(ScanError::UnsupportedBuiltinTarget(format!(
                "architecture {architecture:?}"
            )))
        }
    }
    match target.operating_system() {
        OperatingSystem::Linux | OperatingSystem::Android => {
            define(table, "__linux__", "1");
            define(table, "__unix__", "1");
        }
        OperatingSystem::Darwin | OperatingSystem::MacOs => {
            define(table, "__APPLE__", "1");
            define(table, "__MACH__", "1");
        }
        OperatingSystem::Windows => define(table, "_WIN32", "1"),
        operating_system => {
            return Err(ScanError::UnsupportedBuiltinTarget(format!(
                "operating system {operating_system:?}"
            )))
        }
    }
    if target.endian() == Endian::Little {
        define(table, "__BYTE_ORDER__", "1234");
    } else {
        define(table, "__BYTE_ORDER__", "4321");
    }
    define(table, "__ORDER_LITTLE_ENDIAN__", "1234");
    define(table, "__ORDER_BIG_ENDIAN__", "4321");
    match target.compiler().family() {
        CompilerFamily::Gcc => {
            if let Some(major) = target.compiler().version().split('.').next() {
                if major.bytes().all(|byte| byte.is_ascii_digit()) {
                    define(table, "__GNUC__", major);
                }
            }
        }
        CompilerFamily::Clang | CompilerFamily::AppleClang => define(table, "__clang__", "1"),
        CompilerFamily::Msvc => {}
    }
    Ok(())
}

fn generated_span_range(file: FileId, span: crate::span::Span, len: usize) -> Option<SourceRange> {
    if span.is_none() || span.start > span.end || span.end > len {
        return None;
    }
    Some(SourceRange {
        file,
        start: u64::try_from(span.start).ok()?,
        end: u64::try_from(span.end).ok()?,
    })
}

fn diagnostic_code(value: &str) -> DiagnosticCode {
    DiagnosticCode::new(value).expect("static diagnostic code")
}

fn target_default_visibility(target: &TargetSpec) -> Visibility {
    let mut visibility = Visibility::Unspecified;
    for argument in target.abi_flags() {
        visibility = match argument.as_str() {
            "-fvisibility=default" => Visibility::TargetDefault,
            "-fvisibility=hidden" => Visibility::Hidden,
            "-fvisibility=protected" => Visibility::Protected,
            "-fvisibility=internal" => Visibility::Internal,
            _ => visibility,
        };
    }
    visibility
}

fn completeness_from_diagnostics(diagnostics: &[SourceDiagnostic]) -> Completeness {
    let mut reasons = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.completeness_impact != DiagnosticCompletenessImpact::Informational
        })
        .map(|diagnostic| CompletenessReason {
            code: diagnostic.code.clone(),
            message: diagnostic.message.clone(),
            range: diagnostic.range,
        })
        .collect::<Vec<_>>();
    reasons.sort();
    reasons.dedup();

    if diagnostics.iter().any(|diagnostic| {
        diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesRejected
    }) {
        Completeness::Rejected { reasons }
    } else if reasons.is_empty() {
        Completeness::Complete
    } else {
        Completeness::Partial { reasons }
    }
}
