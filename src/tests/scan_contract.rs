use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ast::{DeclaratorKind, ExternalDeclaration};
use crate::contract::*;
use crate::driver::Flavor;
use crate::extract::{extract_contract, ExtractionContext};
use crate::scan::{
    scan_headers, EnvironmentPolicy, PathMapping, PathMappingError, PathMappingRule,
    PreprocessorMode, ScanConfig, ScanConfigError, ScanError,
};

static FIXTURE_ORDINAL: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    header: PathBuf,
}

impl Fixture {
    fn new(label: &str, source: &str) -> Self {
        let ordinal = FIXTURE_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "parc-h1-scan-{label}-{}-{ordinal}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create scan fixture root");
        let header = root.join("api.h");
        std::fs::write(&header, source).expect("write scan fixture");
        Self { root, header }
    }

    fn config(&self) -> ScanConfig {
        let rule = PathMappingRule::try_new(&self.root, "fixture").expect("path mapping rule");
        let mapping = PathMapping::try_new([rule]).expect("path mapping");
        ScanConfig::new(test_target(), mapping, PreprocessorMode::Builtin)
            .expect("scan config")
            .entry_header(&self.header)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn scan_lowers_two_pass_contract_and_forces_generated_partial() {
    let fixture = Fixture::new(
        "contract",
        r#"
struct parc_node;
typedef struct parc_node *parc_handle;
struct parc_node {
    const char *name;
    unsigned count;
    int values[];
};
enum parc_mode {
    PARC_MODE_A = 1,
    PARC_MODE_B,
    PARC_MODE_WIDE = 18446744073709551615ULL
};
typedef int (*parc_callback)(const int *value);
extern int parc_global;
int parc_tentative;
int parc_call(parc_handle restrict value, const int *items);
int parc_consume(int *restrict value, int items[static 4]);
"#,
    );

    let report = scan_headers(&fixture.config()).expect("scan should produce a checked package");
    let package = report.package();
    assert!(matches!(
        package.completeness(),
        Completeness::Partial { .. }
    ));
    assert!(package.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "PARC-P0001"
            && diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesPartial
    }));
    assert!(package.macros().is_empty());

    let generated = package
        .files()
        .iter()
        .find(|file| file.role == SourceFileRole::Generated)
        .expect("generated source file");
    for declaration in package.declarations() {
        for occurrence in &declaration.occurrences {
            assert_eq!(occurrence.range.file, generated.id);
            assert!(occurrence.range.end <= generated.byte_len);
            assert_eq!(occurrence.provenance.origin, SourceOrigin::Generated);
            assert!(occurrence.provenance.include_chain.is_empty());
            assert!(occurrence.provenance.macro_expansions.is_empty());
        }
    }

    let node = named(package, "parc_node");
    let SourceDeclarationKind::Record(record) = &node.kind else {
        panic!("parc_node must lower as a record");
    };
    assert_eq!(record.completeness, RecordCompleteness::Complete);
    assert_eq!(node.occurrences.len(), 3);
    assert_eq!(
        node.occurrences
            .iter()
            .filter(|occurrence| occurrence.is_definition)
            .count(),
        1
    );
    assert!(matches!(
        record.fields.last().map(|field| &field.ty.kind),
        Some(CTypeKind::Array {
            bound: ArrayBound::Flexible,
            ..
        })
    ));

    let alias = named(package, "parc_handle");
    let SourceDeclarationKind::TypeAlias(alias_kind) = &alias.kind else {
        panic!("parc_handle must lower as a type alias");
    };
    assert!(matches!(alias_kind.target.kind, CTypeKind::Pointer(_)));
    assert!(alias.occurrences[0].is_definition);

    let callback = named(package, "parc_callback");
    let SourceDeclarationKind::TypeAlias(callback_alias) = &callback.kind else {
        panic!("parc_callback must lower as a type alias");
    };
    let CTypeKind::Pointer(callback_pointee) = &callback_alias.target.kind else {
        panic!("callback alias must preserve its pointer layer");
    };
    let CTypeKind::Function(callback_function) = &callback_pointee.kind else {
        panic!("callback alias must preserve its function layer");
    };
    assert_eq!(callback_function.calling_convention, CallingConvention::C);
    assert!(matches!(
        callback_function.parameters[0].ty.kind,
        CTypeKind::Pointer(_)
    ));

    let mode = named(package, "parc_mode");
    let SourceDeclarationKind::Enum(mode_enum) = &mode.kind else {
        panic!("parc_mode must lower as an enum");
    };
    assert!(matches!(
        mode_enum
            .variants
            .iter()
            .find(|variant| variant.name.original == "PARC_MODE_WIDE")
            .map(|variant| &variant.value),
        Some(EnumValue::Evaluated { value })
            if *value == ExactInteger::unsigned(u64::MAX as u128)
    ));

    let global = named(package, "parc_global");
    assert!(!global.occurrences[0].is_definition);
    let tentative = named(package, "parc_tentative");
    assert!(tentative.occurrences[0].is_definition);
    let call = named(package, "parc_call");
    assert!(!call.occurrences[0].is_definition);
    let SourceDeclarationKind::Function(call_function) = &call.kind else {
        panic!("parc_call must lower as a function");
    };
    assert!(call_function.parameters[0].ty.qualifiers.is_restrict);
    let consume = named(package, "parc_consume");
    let SourceDeclarationKind::Function(consume_function) = &consume.kind else {
        panic!("parc_consume must lower as a function");
    };
    assert!(consume_function.parameters[0].ty.qualifiers.is_restrict);
    assert!(matches!(
        &consume_function.parameters[1].ty.kind,
        CTypeKind::Array {
            bound: ArrayBound::StaticMinimum {
                minimum: ArrayMinimumBound::Fixed { elements: 4 }
            },
            ..
        }
    ));
    assert!(consume_function.parameters[1].support.is_supported());

    assert!(report
        .clone()
        .into_complete(&Selection::AllSupported)
        .is_err());
}

#[test]
fn relocated_roots_preserve_contract_identity() {
    let left = Fixture::new("relocate-left", "typedef unsigned long parc_size;\n");
    let right = Fixture::new("relocate-right", "typedef unsigned long parc_size;\n");

    let left_package = scan_headers(&left.config())
        .expect("left scan")
        .into_package();
    let right_package = scan_headers(&right.config())
        .expect("right scan")
        .into_package();
    assert_eq!(left_package.fingerprint(), right_package.fingerprint());
    assert_eq!(
        left_package.declarations()[0].id,
        right_package.declarations()[0].id
    );
}

#[test]
fn config_rejects_ambiguous_roots_and_multiple_translation_units() {
    let fixture = Fixture::new("config", "int first;\n");
    let second = fixture.root.join("second.h");
    std::fs::write(&second, "int second;\n").expect("write second header");
    let first_rule = PathMappingRule::try_new(&fixture.root, "first").expect("first rule");
    let second_rule = PathMappingRule::try_new(&fixture.root, "second").expect("second rule");
    assert!(matches!(
        PathMapping::try_new([first_rule.clone(), second_rule]),
        Err(PathMappingError::DuplicatePhysicalRoot(_))
    ));

    let mapping = PathMapping::try_new([first_rule]).expect("mapping");
    let config = ScanConfig::new(test_target(), mapping, PreprocessorMode::Builtin)
        .expect("config")
        .entry_header(&fixture.header)
        .entry_header(&second);
    assert!(matches!(
        scan_headers(&config),
        Err(ScanError::Configuration(
            ScanConfigError::MultipleEntryHeaders
        ))
    ));
}

#[test]
fn captured_environment_records_requested_unset_variables() {
    let fixture = Fixture::new("environment", "int parc_value;\n");
    let name = format!("PARC_H1_UNSET_{}_{}", std::process::id(), 1);
    std::env::remove_var(&name);
    let policy = EnvironmentPolicy::captured([name.clone()]).expect("captured policy");
    let config = fixture.config().with_environment(policy);
    let package = scan_headers(&config)
        .expect("captured environment scan")
        .into_package();
    let EnvironmentInputs::Captured { variables } = &package.inputs().environment else {
        panic!("environment must be captured");
    };
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].name, name);
}

#[test]
fn captured_environment_rejects_non_identifier_names() {
    for name in ["", "1STARTS_WITH_DIGIT", "lowercase", "HAS-DASH"] {
        assert!(matches!(
            EnvironmentPolicy::captured([name]),
            Err(ScanConfigError::InvalidEnvironmentVariable)
        ));
    }
    EnvironmentPolicy::captured(["_VALID", "VALID_2"]).expect("valid environment names");
}

#[test]
fn recovery_is_structured_and_forces_partial() {
    let fixture = Fixture::new("recovery", "int before;\n@@@invalid@@@;\nint after;\n");
    let package = scan_headers(&fixture.config())
        .expect("recovery scan")
        .into_package();
    assert!(package.diagnostics().iter().any(|diagnostic| {
        diagnostic.stage == DiagnosticStage::Recovery
            && diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesPartial
            && diagnostic.range.is_some()
    }));
    assert!(named_optional(&package, "before").is_some());
    assert!(named_optional(&package, "after").is_some());
}

#[test]
fn nameless_top_level_declarator_is_not_silently_discarded() {
    let source = "int named(void);\n";
    let mut unit = crate::parse::translation_unit(source, Flavor::GnuC11)
        .expect("parse named declaration fixture");
    let ExternalDeclaration::Declaration(declaration) = &mut unit.0[0].node else {
        panic!("fixture must parse as a declaration");
    };
    declaration.node.declarators[0]
        .node
        .declarator
        .node
        .kind
        .node = DeclaratorKind::Abstract;
    let output = extract_contract(
        &unit,
        ExtractionContext {
            source,
            generated_file: FileId::from_logical_path("generated/nameless.c")
                .expect("generated file ID"),
            target: test_target().fingerprint(),
            default_visibility: Visibility::Unspecified,
        },
    );
    assert!(output.declarations.is_empty());
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.code.as_str() == "PARC-E1213"
            && diagnostic.stage == DiagnosticStage::Extract
            && diagnostic.completeness_impact == DiagnosticCompletenessImpact::ForcesRejected
            && diagnostic.range.is_some()
    }));
}

#[test]
fn malformed_visibility_has_matching_rejection_diagnostic() {
    let fixture = Fixture::new(
        "visibility",
        "int bad_visibility(void) __attribute__((visibility(\"mystery\")));\n",
    );
    let package = scan_headers(&fixture.config())
        .expect("malformed visibility scan")
        .into_package();
    let declaration = named(&package, "bad_visibility");
    assert_support_diagnostic(
        &package,
        declaration,
        "PARC-E1215",
        DiagnosticCompletenessImpact::ForcesRejected,
    );
    assert_eq!(declaration.visibility, Visibility::Unspecified);
}

#[test]
fn conflicting_calling_conventions_reject_direct_and_nested_functions() {
    let fixture = Fixture::new(
        "calling-convention",
        r#"
int bad_cc(void) __attribute__((stdcall, cdecl));
typedef int (__attribute__((stdcall)) *bad_callback)(void) __attribute__((cdecl));
"#,
    );
    let package = scan_headers(&fixture.config())
        .expect("conflicting calling convention scan")
        .into_package();
    for name in ["bad_cc", "bad_callback"] {
        let declaration = named(&package, name);
        assert_support_diagnostic(
            &package,
            declaration,
            "PARC-E1214",
            DiagnosticCompletenessImpact::ForcesRejected,
        );
    }
    let callback = named(&package, "bad_callback");
    let SourceDeclarationKind::TypeAlias(alias) = &callback.kind else {
        panic!("bad_callback must remain a type alias");
    };
    let CTypeKind::Pointer(pointee) = &alias.target.kind else {
        panic!("bad_callback must preserve its pointer layer");
    };
    let CTypeKind::Function(function) = &pointee.kind else {
        panic!("bad_callback must preserve its function layer");
    };
    assert!(matches!(
        function.calling_convention,
        CallingConvention::Unsupported { .. }
    ));
}

#[test]
fn unmodeled_abi_attribute_has_matching_partial_diagnostic() {
    let fixture = Fixture::new(
        "unmodeled-attribute",
        "int partial_attr(void) __attribute__((preserve_most));\n",
    );
    let package = scan_headers(&fixture.config())
        .expect("unmodeled attribute scan")
        .into_package();
    let declaration = named(&package, "partial_attr");
    assert_support_diagnostic(
        &package,
        declaration,
        "PARC-P1205",
        DiagnosticCompletenessImpact::ForcesPartial,
    );
}

fn assert_support_diagnostic(
    package: &SourcePackage,
    declaration: &SourceDeclaration,
    expected_code: &str,
    expected_impact: DiagnosticCompletenessImpact,
) {
    let (support_code, support_reason) = match &declaration.support {
        SupportStatus::Partial { code, reason } | SupportStatus::Unsupported { code, reason } => {
            (code, reason)
        }
        SupportStatus::Supported => panic!("{expected_code} must mark the declaration unsupported"),
    };
    assert_eq!(support_code.as_str(), expected_code);
    let diagnostic = package
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_str() == expected_code
                && diagnostic.declaration == Some(declaration.id)
        })
        .unwrap_or_else(|| panic!("missing forcing diagnostic {expected_code}"));
    assert_eq!(diagnostic.message, *support_reason);
    assert_eq!(diagnostic.completeness_impact, expected_impact);
    assert_eq!(
        diagnostic.range,
        declaration
            .occurrences
            .last()
            .map(|occurrence| occurrence.range)
    );
}

fn named<'a>(package: &'a SourcePackage, name: &str) -> &'a SourceDeclaration {
    named_optional(package, name).unwrap_or_else(|| panic!("missing declaration {name}"))
}

fn named_optional<'a>(package: &'a SourcePackage, name: &str) -> Option<&'a SourceDeclaration> {
    package.declarations().iter().find(|declaration| {
        declaration
            .name
            .as_ref()
            .is_some_and(|value| value.original == name)
    })
}

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

fn test_target() -> TargetSpec {
    let triple = "x86_64-unknown-linux-gnu";
    TargetSpec::try_new(TargetSpecParts {
        triple: triple.to_owned(),
        architecture: Architecture::X86_64,
        vendor: Vendor::try_new("unknown").expect("vendor"),
        operating_system: OperatingSystem::Linux,
        environment: Environment::Gnu,
        object_format: ObjectFormat::Elf,
        endian: Endian::Little,
        pointer_width: 64,
        c_data_model: CDataModel {
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
        },
        language_standard: LanguageStandard::C17,
        extension_profile: ExtensionProfile::new(ExtensionFamily::Gnu, []),
        compiler: CompilerIdentity::try_new(
            CompilerFamily::Gcc,
            "toolchains/gcc/bin/gcc",
            ContentFingerprint::from_content(b"scan-test-gcc"),
            ContentFingerprint::from_content(b"scan-test-gcc-version"),
            triple,
            "13.2.0",
        )
        .expect("compiler identity"),
        sysroot: None,
        abi_flags: vec![NormalizedCompilerArg::try_new("-m64").expect("ABI argument")],
    })
    .expect("target")
}
