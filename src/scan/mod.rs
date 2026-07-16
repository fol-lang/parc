//! Explicit-target scan pipeline producing the checked schema-v2 contract.

pub mod config;

pub use config::{
    EnvironmentPolicy, PathMapping, PathMappingError, PathMappingRule, PreprocessorMode,
    ScanConfig, ScanConfigError,
};

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

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
}

/// Scan configured headers. Every declaration range is exact within the
/// generated preprocessed file. Because original include/macro provenance is
/// not yet available, this path always returns `Completeness::Partial`.
pub fn scan_headers(config: &ScanConfig) -> Result<ScanReport, ScanError> {
    config.validate()?;
    if config.entry_headers.is_empty() {
        return Err(ScanError::NoEntryHeaders);
    }

    let environment = capture_environment(&config.environment)?;
    let mut file_table = BTreeMap::new();
    let mut entry_files = Vec::new();
    for path in &config.entry_headers {
        let materialized = materialize_file(config, path, SourceFileRole::Entry)?;
        entry_files.push(materialized.contract.id);
        file_table.insert(materialized.contract.id, materialized.contract);
    }
    let mut forced_includes = Vec::new();
    for path in &config.forced_includes {
        let materialized = materialize_file(config, path, SourceFileRole::UserInclude)?;
        forced_includes.push(materialized.contract.id);
        file_table
            .entry(materialized.contract.id)
            .or_insert(materialized.contract);
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

    let preprocessed = match &config.preprocessor {
        PreprocessorMode::Builtin => preprocess_builtin(config, &environment)?,
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
    let extracted = extract_contract(
        &recovered.unit,
        ExtractionContext {
            source: &preprocessed.text,
            generated_file: generated_id,
            target: config.target.fingerprint(),
            default_visibility: target_default_visibility(&config.target),
        },
    );

    let generated_range = SourceRange {
        file: generated_id,
        start: 0,
        end: u64::try_from(preprocessed.text.len()).map_err(|_| ScanError::SizeOverflow)?,
    };
    let generated_reason = CompletenessReason {
        code: diagnostic_code(GENERATED_PROVENANCE_CODE),
        message: "declarations refer to exact generated-source ranges; original include and macro provenance is not yet provable".to_owned(),
        range: Some(generated_range),
    };
    let mut diagnostics = extracted.diagnostics;
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
    for recovery in recovered.errors {
        let range = generated_span_range(generated_id, recovery.skipped, preprocessed.text.len());
        let message = format!(
            "parser recovery skipped bytes after error at {}:{}: {:?}",
            recovery.error.line, recovery.error.column, recovery.error.expected
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
        macros: Vec::new(),
        diagnostics,
        completeness,
    };
    Ok(ScanReport::new(SourcePackage::try_new(input)?))
}

fn materialize_file(
    config: &ScanConfig,
    path: &Path,
    role: SourceFileRole,
) -> Result<MaterializedFile, ScanError> {
    let content = std::fs::read(path).map_err(|source| ScanError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let logical_path = config.path_mapping.map_path(path)?;
    let id = FileId::from_logical_path(&logical_path)
        .expect("PathMapping returns canonical logical paths");
    Ok(MaterializedFile {
        contract: source_file(id, logical_path, role, &content)?,
    })
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

    let executable_bytes = std::fs::read(executable).map_err(|source| ScanError::Read {
        path: executable.display().to_string(),
        source,
    })?;
    let executable_fingerprint = ContentFingerprint::from_content(&executable_bytes);
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
        .env_clear();
    for (name, value) in &environment.values {
        command.env(name, value);
    }
    let output = command
        .output()
        .map_err(|error| ScanError::ExternalPreprocessor(error.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)
            .map_err(|error| ScanError::NonUtf8Output(error.to_string()))?;
        return Err(ScanError::ExternalPreprocessor(stderr));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|error| ScanError::NonUtf8Output(error.to_string()))?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|error| ScanError::NonUtf8Output(error.to_string()))?;
    Ok(Preprocessed {
        text,
        identity: PreprocessorIdentity::External {
            executable: config.target.compiler().logical_executable().to_owned(),
            executable_fingerprint,
            arguments,
        },
        warnings: if stderr.is_empty() {
            Vec::new()
        } else {
            vec![stderr]
        },
    })
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

fn preprocess_builtin(
    config: &ScanConfig,
    environment: &EnvironmentCapture,
) -> Result<Preprocessed, ScanError> {
    use crate::preprocess::{
        builtin_headers, IncludeResolver, MacroDef, MacroTable, Processor, TokenKind,
    };

    let mut macros = MacroTable::new();
    define_builtin_target_macros(&mut macros, &config.target)?;
    for event in &config.define_events {
        match event {
            DefineEvent::Define { name, value } => {
                let body_text = value.as_deref().unwrap_or("1");
                let body = crate::preprocess::Lexer::tokenize(body_text)
                    .into_iter()
                    .filter(|token| token.kind != TokenKind::Eof)
                    .collect();
                macros.define(MacroDef {
                    name: name.clone(),
                    params: None,
                    is_variadic: false,
                    body,
                });
            }
            DefineEvent::Undefine { name } => macros.undef(name),
        }
    }
    let mut processor = Processor::with_macros(macros);
    let mut resolver = IncludeResolver::new();
    resolver.register_builtin_headers(builtin_headers());
    for directory in &config.include_dirs {
        resolver.add_local_path(directory);
        resolver.add_system_path(directory);
    }
    for directory in &config.system_include_dirs {
        resolver.add_system_path(directory);
        resolver.add_local_path(directory);
    }
    for (directory, _) in &environment.include_paths {
        resolver.add_system_path(directory);
        resolver.add_local_path(directory);
    }

    let mut text = String::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for path in config
        .forced_includes
        .iter()
        .chain(config.entry_headers.iter())
    {
        let result = resolver.preprocess_file(path, &mut processor);
        text.push_str(&result.text);
        text.push('\n');
        errors.extend(result.errors);
        warnings.extend(result.warnings);
    }
    if !errors.is_empty() {
        return Err(ScanError::BuiltinPreprocessor(errors.join("\n")));
    }
    Ok(Preprocessed {
        text,
        identity: PreprocessorIdentity::Builtin {
            implementation_version: BUILTIN_VERSION.to_owned(),
        },
        warnings,
    })
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
