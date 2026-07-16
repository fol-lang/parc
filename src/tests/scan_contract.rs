use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::ast::{DeclaratorKind, ExternalDeclaration};
use crate::contract::*;
use crate::driver::Flavor;
use crate::extract::{extract_contract, ExtractionContext};
use crate::scan::{
    scan_headers, EnvironmentPolicy, PathMapping, PathMappingError, PathMappingRule,
    PreprocessorMode, ScanConfig, ScanConfigError, ScanError, ScanLimits,
};

static FIXTURE_ORDINAL: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    header: PathBuf,
}

impl Fixture {
    fn new(label: &str, source: &str) -> Self {
        let ordinal = FIXTURE_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "parc-h2-scan-{label}-{}-{ordinal}-{nonce}",
            std::process::id(),
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

    fn write(&self, relative: &str, source: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create nested fixture directory");
        }
        std::fs::write(&path, source).expect("write nested scan fixture");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn scan_lowers_two_pass_contract_with_complete_original_provenance() {
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
    assert_eq!(package.completeness(), &Completeness::Complete);
    assert!(package.diagnostics().iter().all(|diagnostic| {
        diagnostic.completeness_impact == DiagnosticCompletenessImpact::Informational
    }));
    assert!(!package.macros().is_empty());

    assert!(package
        .files()
        .iter()
        .all(|file| file.role != SourceFileRole::Generated));
    let entry = package
        .files()
        .iter()
        .find(|file| file.role == SourceFileRole::Entry)
        .expect("entry source file");
    for declaration in package.declarations() {
        for occurrence in &declaration.occurrences {
            assert_eq!(occurrence.range.file, entry.id);
            assert!(occurrence.range.end <= entry.byte_len);
            assert_eq!(occurrence.provenance.origin, SourceOrigin::Entry);
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

    report
        .clone()
        .into_complete(&Selection::AllSupported)
        .expect("ordinary certified scan must be accepted by strict consumers");
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
fn distinct_builtin_translation_units_merge_without_generated_file_conflicts() {
    let fixture = Fixture::new("merge-left", "int parc_left(void);\n");
    let right_header = fixture.write("right.h", "long parc_right(unsigned value);\n");
    let mapping = PathMapping::try_new([
        PathMappingRule::try_new(&fixture.root, "fixture").expect("path mapping rule")
    ])
    .expect("path mapping");
    let left_config = ScanConfig::new(test_target(), mapping.clone(), PreprocessorMode::Builtin)
        .expect("left scan config")
        .entry_header(&fixture.header);
    let right_config = ScanConfig::new(test_target(), mapping, PreprocessorMode::Builtin)
        .expect("right scan config")
        .entry_header(&right_header);

    let left = scan_headers(&left_config)
        .expect("left scan")
        .into_package();
    let right = scan_headers(&right_config)
        .expect("right scan")
        .into_package();
    assert_eq!(left.completeness(), &Completeness::Complete);
    assert_eq!(right.completeness(), &Completeness::Complete);

    let merged = left.merge(right).expect("merge independent scans");
    assert_eq!(merged.completeness(), &Completeness::Complete);
    assert_eq!(merged.inputs().entry_files.len(), 2);
    assert!(named_optional(&merged, "parc_left").is_some());
    assert!(named_optional(&merged, "parc_right").is_some());
    assert!(merged
        .files()
        .iter()
        .all(|file| file.role != SourceFileRole::Generated));
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
    let name = format!("PARC_H2_UNSET_{}_{}", std::process::id(), 1);
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
            int128_supported: true,
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

#[test]
fn transitive_files_declarations_macros_and_expansions_have_original_provenance() {
    let fixture = Fixture::new(
        "transitive-provenance",
        "#include \"dep.h\"\nint entry_value = DEP_VALUE;\n",
    );
    fixture.write(
        "dep.h",
        "#ifndef DEP_H\n#define DEP_H\n#define DEP_VALUE 7\nint dep_value;\n#endif\n",
    );
    let package = scan_headers(&fixture.config())
        .expect("transitive scan")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);

    let dep_file = package
        .files()
        .iter()
        .find(|file| file.logical_path == "fixture/dep.h")
        .expect("transitive include file");
    assert_eq!(dep_file.role, SourceFileRole::UserInclude);
    let dep = named(&package, "dep_value");
    assert_eq!(dep.occurrences[0].range.file, dep_file.id);
    assert_eq!(
        dep.occurrences[0].provenance.origin,
        SourceOrigin::UserInclude
    );
    assert_eq!(dep.occurrences[0].provenance.include_chain.len(), 1);

    let entry = named(&package, "entry_value");
    assert_eq!(entry.occurrences[0].provenance.origin, SourceOrigin::Entry);
    assert!(entry.occurrences[0]
        .provenance
        .macro_expansions
        .iter()
        .any(|expansion| {
            expansion.macro_name == "DEP_VALUE"
                && expansion
                    .definition
                    .is_some_and(|range| range.file == dep_file.id)
        }));
    let macro_item = package
        .macros()
        .iter()
        .find(|macro_item| macro_item.name == "DEP_VALUE")
        .expect("effective macro inventory");
    assert_eq!(macro_item.identity_file, dep_file.id);
    assert!(macro_item.support.is_supported());
    assert!(matches!(macro_item.value, Some(MacroValue::Integer { .. })));
}

#[test]
fn warnings_are_structured_ranged_and_informational() {
    let fixture = Fixture::new("warning", "#warning check this\nint warned;\n");
    let package = scan_headers(&fixture.config())
        .expect("warning scan")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);
    let warning = package
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == "PARC-W2100")
        .expect("structured preprocessor warning");
    assert_eq!(warning.stage, DiagnosticStage::Preprocess);
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(
        warning.completeness_impact,
        DiagnosticCompletenessImpact::Informational
    );
    let range = warning.range.expect("warning source range");
    assert!(package
        .files()
        .iter()
        .any(|file| file.id == range.file && file.role == SourceFileRole::Entry));
}

#[test]
fn unsupported_directives_pragmas_line_markers_and_midline_hash_fail_closed() {
    for (label, source, code, rejected) in [
        ("missing", "#include \"absent.h\"\n", "PARC-P2100", false),
        ("unknown", "#frobnicate value\n", "PARC-P2101", false),
        ("next", "#include_next <stdint.h>\n", "PARC-P2102", false),
        ("pragma", "#pragma vendor_magic\n", "PARC-P2103", false),
        ("line", "#line 42 \"other.h\"\n", "PARC-P2104", false),
        ("pack", "#pragma pack(push, 1)\n", "PARC-E2104", true),
        (
            "midline",
            "int before_hash; #include \"absent.h\"\n",
            "PARC-E2108",
            true,
        ),
        ("absolute", "#include \"/etc/passwd\"\n", "PARC-E2102", true),
    ] {
        let fixture = Fixture::new(label, source);
        let package = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("{label} scan failed operationally: {error}"))
            .into_package();
        assert_diagnostic(&package, code);
        assert_eq!(
            matches!(package.completeness(), Completeness::Rejected { .. }),
            rejected,
            "wrong completeness for {label}"
        );
    }
}

#[test]
fn malformed_conditionals_and_nonexact_if_expressions_are_rejected() {
    for (label, expression) in [
        ("division-zero", "1 / 0"),
        ("overflow", "9223372036854775807L + 1L"),
        ("negative-shift", "1 << -1"),
        ("wide-shift", "1 << 64"),
        ("ternary", "1 ? 2"),
        ("paren", "(1 + 2"),
        ("unsigned", "1U < 2U"),
        ("complement", "~0"),
    ] {
        let fixture = Fixture::new(label, &format!("#if {expression}\nint chosen;\n#endif\n"));
        let package = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("{label} scan: {error}"))
            .into_package();
        assert_diagnostic(&package, "PARC-E2113");
        assert!(matches!(
            package.completeness(),
            Completeness::Rejected { .. }
        ));
    }

    let valid = Fixture::new(
        "checked-signed-if",
        "#if (-9223372036854775807L - 1L) < 0L\nint chosen;\n#endif\n",
    );
    let package = scan_headers(&valid.config())
        .expect("checked signed conditional")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);
    assert!(named_optional(&package, "chosen").is_some());

    for (label, source) in [
        ("duplicate-else", "#if 1\n#else\n#else\n#endif\n"),
        ("elif-after-else", "#if 0\n#else\n#elif 1\n#endif\n"),
    ] {
        let fixture = Fixture::new(label, source);
        let package = scan_headers(&fixture.config())
            .expect("malformed conditional scan")
            .into_package();
        assert_diagnostic(&package, "PARC-E2107");
    }
}

#[test]
fn malformed_macro_invocations_and_operators_are_rejected() {
    for (label, source, code) in [
        (
            "macro-arity",
            "#define ADD(a,b) ((a)+(b))\nint value = ADD(1);\n",
            "PARC-E2110",
        ),
        (
            "macro-hash",
            "#define STRINGIFY(x) #x\nint value;\n",
            "PARC-E2111",
        ),
        (
            "macro-paste",
            "#define PASTE(a,b) a ## b\nint value;\n",
            "PARC-E2111",
        ),
        (
            "macro-unclosed",
            "#define ID(x) x\nint value = ID(1;\n",
            "PARC-E2112",
        ),
    ] {
        let fixture = Fixture::new(label, source);
        let package = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("{label} scan: {error}"))
            .into_package();
        assert_diagnostic(&package, code);
        assert!(matches!(
            package.completeness(),
            Completeness::Rejected { .. }
        ));
    }
}

#[test]
fn guarded_recursive_includes_complete_and_unguarded_cycles_hit_depth_budget() {
    let self_guarded = Fixture::new(
        "self-guarded",
        "#ifndef SELF_H\n#define SELF_H\n#include \"api.h\"\nint self_ok;\n#endif\n",
    );
    let package = scan_headers(&self_guarded.config())
        .expect("guarded self include")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);
    assert!(named_optional(&package, "self_ok").is_some());

    let mutual = Fixture::new(
        "mutual-guarded",
        "#ifndef A_H\n#define A_H\n#include \"b.h\"\nint from_a;\n#endif\n",
    );
    mutual.write(
        "b.h",
        "#ifndef B_H\n#define B_H\n#include \"api.h\"\nint from_b;\n#endif\n",
    );
    let package = scan_headers(&mutual.config())
        .expect("mutually guarded include")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);
    assert!(named_optional(&package, "from_a").is_some());
    assert!(named_optional(&package, "from_b").is_some());

    let unguarded = Fixture::new("unguarded", "#include \"api.h\"\n");
    let mut limits = ScanLimits::production();
    limits.max_include_depth = 3;
    let config = unguarded
        .config()
        .with_limits(limits)
        .expect("tight include depth");
    let package = scan_headers(&config)
        .expect("unguarded cycle is a checked rejected artifact")
        .into_package();
    assert_diagnostic(&package, "PARC-E2203");
    assert!(matches!(
        package.completeness(),
        Completeness::Rejected { .. }
    ));
}

#[test]
fn effective_macro_inventory_excludes_undefined_and_superseded_definitions() {
    let fixture = Fixture::new(
        "macro-inventory",
        "#include \"first.h\"\n#include \"second.h\"\n#include \"final.h\"\nint value = ACTIVE;\n",
    );
    fixture.write("first.h", "#define ACTIVE 1\n#define GONE 9\n");
    fixture.write("second.h", "#undef ACTIVE\n#undef GONE\n");
    fixture.write("final.h", "#define ACTIVE 2\n");
    let package = scan_headers(&fixture.config())
        .expect("effective inventory scan")
        .into_package();
    assert_eq!(package.completeness(), &Completeness::Complete);
    assert!(!package.macros().iter().any(|item| item.name == "GONE"));
    let active = package
        .macros()
        .iter()
        .filter(|item| item.name == "ACTIVE")
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(
        active[0].identity_file,
        mapped_file_id_for_test("fixture/final.h")
    );
    assert!(matches!(active[0].value, Some(MacroValue::Integer { .. })));

    let conflict = Fixture::new(
        "macro-conflict",
        "#include \"one.h\"\n#include \"two.h\"\nint value = CLASH;\n",
    );
    conflict.write("one.h", "#define CLASH 1\n");
    conflict.write("two.h", "#define CLASH 2\n");
    let package = scan_headers(&conflict.config())
        .expect("conflicting macro scan")
        .into_package();
    assert_diagnostic(&package, "PARC-P2110");
    assert!(matches!(
        package.completeness(),
        Completeness::Partial { .. }
    ));
    let clash = package
        .macros()
        .iter()
        .filter(|item| item.name == "CLASH")
        .collect::<Vec<_>>();
    assert_eq!(clash.len(), 1);
    assert!(matches!(clash[0].support, SupportStatus::Partial { .. }));
}

#[test]
fn resource_limits_are_bounded_fail_closed_and_identity_neutral_below_the_ceiling() {
    let fixture = Fixture::new(
        "limits-neutral",
        "#define VALUE 3\nint first = VALUE;\nint second = VALUE;\n",
    );
    let production = scan_headers(&fixture.config())
        .expect("production-limit scan")
        .into_package();
    let mut tighter = ScanLimits::production();
    tighter.max_input_file_bytes = 1_024;
    tighter.max_total_input_bytes = 2_048;
    tighter.max_include_depth = 8;
    tighter.max_include_count = 16;
    tighter.max_macro_definitions = 128;
    tighter.max_macro_expansions = 16;
    tighter.max_macro_expansion_depth = 8;
    tighter.max_tokens = 256;
    tighter.max_generated_bytes = 1_024;
    let tighter_package = scan_headers(
        &fixture
            .config()
            .with_limits(tighter)
            .expect("valid tighter limits"),
    )
    .expect("below-limit scan")
    .into_package();
    assert_eq!(production.fingerprint(), tighter_package.fingerprint());

    let oversized = Fixture::new("file-limit", "int value_with_long_spelling;\n");
    let mut limits = ScanLimits::production();
    limits.max_input_file_bytes = 8;
    let error = scan_headers(
        &oversized
            .config()
            .with_limits(limits)
            .expect("file limit config"),
    )
    .expect_err("oversized source must fail before allocation");
    assert!(matches!(
        error,
        ScanError::ResourceLimit {
            code: "PARC-E2201",
            ..
        }
    ));

    let total = Fixture::new(
        "total-limit",
        "#include \"a.h\"\n#include \"b.h\"\nint root;\n",
    );
    total.write("a.h", "int aaaaaaaaaa;\n");
    total.write("b.h", "int bbbbbbbbbb;\n");
    let mut limits = ScanLimits::production();
    limits.max_total_input_bytes = 64;
    let error = scan_headers(
        &total
            .config()
            .with_limits(limits)
            .expect("total limit config"),
    )
    .expect_err("transitive byte total must be bounded");
    assert!(matches!(
        error,
        ScanError::ResourceLimit {
            code: "PARC-E2202",
            ..
        }
    ));

    for (label, source, configure, code) in [
        (
            "include-count",
            "#include \"a.h\"\n#include \"b.h\"\n",
            0_u8,
            "PARC-E2204",
        ),
        (
            "macro-definitions",
            "#define A 1\n#define B 2\nint x;\n",
            1,
            "PARC-E2208",
        ),
        (
            "macro-expansions",
            "#define A 1\nint x=A; int y=A;\n",
            2,
            "PARC-E2206",
        ),
        ("tokens", "int one; int two; int three;\n", 3, "PARC-E2205"),
        ("generated", "int generated_output;\n", 4, "PARC-E2207"),
    ] {
        let fixture = Fixture::new(label, source);
        if label == "include-count" {
            fixture.write("a.h", "int a;\n");
            fixture.write("b.h", "int b;\n");
        }
        let mut limits = ScanLimits::production();
        match configure {
            0 => limits.max_include_count = 1,
            1 => limits.max_macro_definitions = 1,
            2 => limits.max_macro_expansions = 1,
            3 => limits.max_tokens = 5,
            4 => limits.max_generated_bytes = 4,
            _ => unreachable!(),
        }
        let package = scan_headers(
            &fixture
                .config()
                .with_limits(limits)
                .expect("tight resource limits"),
        )
        .unwrap_or_else(|error| panic!("{label} scan failed operationally: {error}"))
        .into_package();
        assert_diagnostic(&package, code);
        assert!(matches!(
            package.completeness(),
            Completeness::Rejected { .. }
        ));
    }
}

#[test]
fn long_acyclic_macro_chain_is_rejected_at_the_depth_ceiling() {
    let mut source = String::new();
    for index in 0..1_024 {
        source.push_str(&format!("#define CHAIN_{index} CHAIN_{}\n", index + 1));
    }
    source.push_str("#define CHAIN_1024 1\nint value = CHAIN_0;\n");
    let fixture = Fixture::new("macro-expansion-depth", &source);
    let mut limits = ScanLimits::production();
    limits.max_macro_expansion_depth = 32;

    let package = scan_headers(
        &fixture
            .config()
            .with_limits(limits)
            .expect("bounded macro expansion depth"),
    )
    .expect("a deep acyclic macro chain must yield a checked artifact")
    .into_package();

    assert_diagnostic(&package, "PARC-E2209");
    assert!(matches!(
        package.completeness(),
        Completeness::Rejected { .. }
    ));

    let mut zero = ScanLimits::production();
    zero.max_macro_expansion_depth = 0;
    assert!(matches!(
        fixture.config().with_limits(zero),
        Err(ScanConfigError::InvalidLimits)
    ));

    let mut above_production = ScanLimits::production();
    above_production.max_macro_expansion_depth =
        ScanLimits::production().max_macro_expansion_depth + 1;
    assert!(matches!(
        fixture.config().with_limits(above_production),
        Err(ScanConfigError::InvalidLimits)
    ));
}

#[cfg(unix)]
#[test]
fn include_symlinks_may_not_escape_explicit_mapping_roots() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("symlink-escape", "#include \"escape.h\"\n");
    let outside = std::env::temp_dir().join(format!(
        "parc-h2-outside-{}-{}",
        std::process::id(),
        FIXTURE_ORDINAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&outside, "int escaped;\n").expect("outside include");
    symlink(&outside, fixture.root.join("escape.h")).expect("escaping include symlink");
    let result = scan_headers(&fixture.config());
    let _ = std::fs::remove_file(&outside);
    assert!(matches!(
        result,
        Err(ScanError::PathMapping(PathMappingError::UnmappedPath(_)))
    ));
}

#[test]
fn exact_type_failures_have_correlated_forcing_diagnostics() {
    for (label, source, name, code, rejected, diagnostic_offsets) in [
        (
            "negative-array",
            "int values[-1];\n",
            "values",
            "PARC-E1103",
            true,
            None,
        ),
        (
            "negative-static-array",
            "void call(int values[static -1]);\n",
            "call",
            "PARC-E1103",
            true,
            None,
        ),
        (
            "negative-bitfield",
            "struct bits { unsigned value : -1; };\n",
            "bits",
            "PARC-E1207",
            true,
            None,
        ),
        (
            "unevaluated-enum",
            "enum mode { MODE = unknown_value };\n",
            "mode",
            "PARC-P1203",
            false,
            None,
        ),
        (
            "packed-record",
            "struct packed_record { int value; } packed_value __attribute__((packed));\n",
            "packed_value",
            "PARC-P1205",
            false,
            None,
        ),
        (
            "aligned-variable",
            "_Alignas(16) int aligned_value;\n",
            "aligned_value",
            "PARC-E1216",
            true,
            Some((0, 12)),
        ),
    ] {
        let fixture = Fixture::new(label, source);
        let package = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("{label} scan: {error}"))
            .into_package();
        let declaration = named(&package, name);
        let diagnostic = assert_support_diagnostic(
            &package,
            declaration,
            code,
            if rejected {
                DiagnosticCompletenessImpact::ForcesRejected
            } else {
                DiagnosticCompletenessImpact::ForcesPartial
            },
        );
        if let Some((start, end)) = diagnostic_offsets {
            let range = diagnostic
                .range
                .expect("explicit construct diagnostic range");
            assert_eq!((range.start, range.end), (start, end));
        }
    }
}

#[test]
fn scan_is_deterministic_across_independent_processes_and_directories() {
    const CHILD_ROOT: &str = "PARC_H2_DETERMINISM_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = PathBuf::from(root);
        let header = root.join("api.h");
        let mapping = PathMapping::try_new([
            PathMappingRule::try_new(&root, "fixture").expect("child mapping rule")
        ])
        .expect("child mapping");
        let config = ScanConfig::new(test_target(), mapping, PreprocessorMode::Builtin)
            .expect("child config")
            .entry_header(header);
        let package = scan_headers(&config).expect("child scan").into_package();
        println!("PARC_H2_FINGERPRINT={}", package.fingerprint());
        return;
    }

    let left = Fixture::new(
        "process-left",
        "#define COUNT 4\nstruct item { int values[COUNT]; };\n",
    );
    let right = Fixture::new(
        "process-right",
        "#define COUNT 4\nstruct item { int values[COUNT]; };\n",
    );
    let executable = std::env::current_exe().expect("current test executable");
    let mut fingerprints = Vec::new();
    for root in [&left.root, &right.root] {
        let output = std::process::Command::new(&executable)
            .args([
                "--exact",
                "tests::scan_contract::scan_is_deterministic_across_independent_processes_and_directories",
                "--nocapture",
            ])
            .env(CHILD_ROOT, root)
            .output()
            .expect("spawn deterministic child scan");
        assert!(
            output.status.success(),
            "child scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("child output UTF-8");
        let fingerprint = stdout
            .lines()
            .find_map(|line| line.strip_prefix("PARC_H2_FINGERPRINT="))
            .expect("child fingerprint marker")
            .to_owned();
        fingerprints.push(fingerprint);
    }
    assert_eq!(fingerprints[0], fingerprints[1]);
}

#[test]
fn seeded_parser_preprocessor_boundary_corpus_is_panic_free_and_deterministic() {
    let fragments = [
        "#if 1\nint ok;\n#endif\n",
        "#if (1 + 2) == 3\nint arithmetic;\n#endif\n",
        "#if 1 / 0\nint bad;\n#endif\n",
        "#if 1 ? 2\nint bad;\n#endif\n",
        "#define F(x) x\nint value = F(1);\n",
        "#define F(x,y) x\nint value = F(1);\n",
        "struct x { unsigned bits : -1; };\n",
        "int before; @@@; int after;\n",
        "#pragma vendor_specific\nint value;\n",
        "int x; #error midline\n",
    ];
    let mut state = 0x5eed_cafe_u64;
    for ordinal in 0..64 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let first = fragments[(state as usize) % fragments.len()];
        state ^= state.rotate_left(17);
        let second = fragments[(state as usize) % fragments.len()];
        let source = format!("{first}\n{second}\n/* seed {state:016x} */\n");
        let fixture = Fixture::new(&format!("seeded-{ordinal}"), &source);
        let left = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("seed {ordinal} left scan: {error}"))
            .into_package();
        let right = scan_headers(&fixture.config())
            .unwrap_or_else(|error| panic!("seed {ordinal} right scan: {error}"))
            .into_package();
        assert_eq!(left.fingerprint(), right.fingerprint(), "seed {ordinal}");
    }
}

#[cfg(unix)]
#[test]
fn external_failures_and_all_execution_ceilings_return_bounded_rejected_artifacts() {
    let cases = [
        (
            "external-nonzero",
            "#!/bin/sh\nexit 7\n",
            "PARC-E2213",
            Duration::from_secs(1),
            1_024_u64,
        ),
        (
            "external-nonutf8",
            "#!/bin/sh\nprintf '\\377'\n",
            "PARC-E2214",
            Duration::from_secs(1),
            1_024,
        ),
        (
            "external-timeout",
            "#!/bin/sh\n/bin/sleep 5\n",
            "PARC-E2210",
            Duration::from_millis(40),
            1_024,
        ),
        (
            "external-output",
            "#!/bin/sh\nwhile :; do printf '0123456789abcdef'; done\n",
            "PARC-E2211",
            Duration::from_secs(1),
            64,
        ),
        (
            "external-descendant-pipe",
            "#!/bin/sh\n(/bin/sleep 5) &\nprintf 'int held_pipe;\\n'\nexit 0\n",
            "PARC-E2212",
            Duration::from_secs(1),
            1_024,
        ),
    ];
    for (label, script_body, code, timeout, output_limit) in cases {
        let fixture = Fixture::new(label, "int original;\n");
        let script = fixture.write("tool.sh", script_body);
        make_executable(&script);
        let mut limits = ScanLimits::production();
        limits.external_timeout = timeout;
        limits.max_external_output_bytes = output_limit;
        let config = external_script_config(&fixture, &script)
            .with_limits(limits)
            .expect("external test limits");
        let started = Instant::now();
        let package = scan_headers(&config)
            .unwrap_or_else(|error| panic!("{label} returned an operational error: {error}"))
            .into_package();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{label} did not return within its bounded cleanup window"
        );
        assert_diagnostic(&package, code);
        assert!(matches!(
            package.completeness(),
            Completeness::Rejected { .. }
        ));
    }
}

#[cfg(unix)]
#[test]
fn external_identity_records_exact_tool_arguments_sysroot_environment_and_inputs() {
    const CAPTURED: &str = "PARC_H2_EXTERNAL_CAPTURE";
    let fixture = Fixture::new("external-evidence", "int external_value;\n");
    let forced = fixture.write("forced.h", "#define FORCED 1\n");
    let include = fixture.root.join("include");
    std::fs::create_dir_all(&include).expect("include directory");
    let sysroot = fixture.root.join("sysroot");
    std::fs::create_dir_all(&sysroot).expect("sysroot directory");
    let script = fixture.write(
        "tool.sh",
        "#!/bin/sh\nfor argument do last=$argument; done\n/bin/cat \"$last\"\n",
    );
    make_executable(&script);
    std::env::set_var(CAPTURED, "captured-value");

    let bytes = std::fs::read(&script).expect("script bytes");
    let compiler = CompilerIdentity::try_new(
        CompilerFamily::Gcc,
        "toolchains/test/bin/cc",
        ContentFingerprint::from_content(&bytes),
        ContentFingerprint::from_content(b"test-tool-version"),
        "x86_64-unknown-linux-gnu",
        "test-1",
    )
    .expect("test compiler identity");
    let sysroot_identity = SysrootIdentity::try_new(
        "toolchains/test/sysroot",
        ContentFingerprint::from_content(b"test-sysroot"),
    )
    .expect("sysroot identity");
    let target = target_with_compiler_and_sysroot(
        "x86_64-unknown-linux-gnu",
        compiler,
        Some(sysroot_identity),
    );
    let mapping = PathMapping::try_new([
        PathMappingRule::try_new(&fixture.root, "fixture").expect("mapping rule")
    ])
    .expect("mapping");
    let config = ScanConfig::new(
        target,
        mapping,
        PreprocessorMode::External { executable: script },
    )
    .expect("external config")
    .entry_header(&fixture.header)
    .forced_include(&forced)
    .include_dir(&include)
    .define("FEATURE", Some("7".to_owned()))
    .undefine("OLD_FEATURE")
    .with_external_sysroot(&sysroot)
    .with_environment(EnvironmentPolicy::captured([CAPTURED]).expect("captured policy"));
    let package = scan_headers(&config)
        .expect("external evidence scan")
        .into_package();
    std::env::remove_var(CAPTURED);
    assert!(matches!(
        package.completeness(),
        Completeness::Partial { .. }
    ));
    assert_diagnostic(&package, "PARC-P0001");
    let PreprocessorIdentity::External {
        executable,
        executable_fingerprint,
        arguments,
    } = &package.inputs().preprocessor
    else {
        panic!("external identity");
    };
    assert_eq!(executable, "toolchains/test/bin/cc");
    assert_eq!(
        *executable_fingerprint,
        package.target().compiler().executable_content()
    );
    for expected in [
        "-E",
        "-P",
        "-std=gnu17",
        "--sysroot",
        "toolchains/test/sysroot",
        "-m64",
        "-I",
        "fixture/include",
        "-include",
        "fixture/forced.h",
        "-DFEATURE=7",
        "-UOLD_FEATURE",
        "fixture/api.h",
    ] {
        assert!(
            arguments.iter().any(|argument| argument == expected),
            "{expected}"
        );
    }
    assert_eq!(
        package.inputs().forced_includes,
        vec![mapped_file_id_for_test("fixture/forced.h")]
    );
    assert!(matches!(
        package.inputs().environment,
        EnvironmentInputs::Captured { ref variables }
            if variables.len() == 1 && variables[0].name == CAPTURED
    ));
    assert_eq!(package.inputs().include_search.len(), 1);
    assert_eq!(
        package.inputs().include_search[0].logical_path,
        "fixture/include"
    );
    assert!(package.inputs().include_search[0].content.is_none());
    assert_eq!(
        package
            .target()
            .sysroot()
            .map(SysrootIdentity::logical_path),
        Some("toolchains/test/sysroot")
    );
}

#[cfg(feature = "system-tests")]
#[test]
fn certified_builtin_and_gcc_clang_external_scans_agree_or_report_exact_differences() {
    const TEST_NAME: &str =
        "certified_builtin_and_gcc_clang_external_scans_agree_or_report_exact_differences";
    assert_eq!(
        certified_requested_target("x86_64-unknown-linux-gnu"),
        Some("x86_64-unknown-linux-gnu")
    );
    assert_eq!(
        certified_requested_target("x86_64-linux-gnu"),
        Some("x86_64-unknown-linux-gnu")
    );
    assert_eq!(
        certified_requested_target("x86_64-pc-linux-gnu"),
        Some("x86_64-unknown-linux-gnu")
    );
    for unsupported in [
        "x86_64-redhat-linux-gnu",
        "amd64-linux-gnu",
        "aarch64-linux-gnu",
    ] {
        assert_eq!(certified_requested_target(unsupported), None);
    }
    let gcc = find_system_executable("gcc");
    let clang = find_system_executable("clang");
    if !crate::tests::system_support::begin_system_test(
        TEST_NAME,
        gcc.is_some() && clang.is_some(),
        "both gcc and clang on the certified x86_64 Linux platform",
    ) {
        return;
    }
    let fixture = Fixture::new(
        "compiler-differential",
        r#"
#if __SIZEOF_LONG__ == 8
typedef unsigned long parc_word;
#else
typedef unsigned long long parc_word;
#endif
struct parc_pair { parc_word left; parc_word right; };
typedef int (*parc_callback)(const struct parc_pair *pair);
int parc_apply(parc_callback callback, const struct parc_pair *pair);
"#,
    );

    for (name, family, executable) in [
        ("gcc", CompilerFamily::Gcc, gcc.expect("checked GCC")),
        (
            "clang",
            CompilerFamily::Clang,
            clang.expect("checked Clang"),
        ),
    ] {
        let target = system_compiler_target(&executable, family);
        let rule = PathMappingRule::try_new(&fixture.root, "fixture").expect("mapping rule");
        let builtin = ScanConfig::new(
            target.clone(),
            PathMapping::try_new([rule.clone()]).expect("builtin mapping"),
            PreprocessorMode::Builtin,
        )
        .expect("builtin config")
        .entry_header(&fixture.header);
        let external = ScanConfig::new(
            target,
            PathMapping::try_new([rule]).expect("external mapping"),
            PreprocessorMode::External {
                executable: executable.clone(),
            },
        )
        .expect("external config")
        .entry_header(&fixture.header);
        let builtin = scan_headers(&builtin)
            .unwrap_or_else(|error| panic!("{name} target builtin scan: {error}"))
            .into_package();
        let external = scan_headers(&external)
            .unwrap_or_else(|error| panic!("{name} external scan: {error}"))
            .into_package();
        assert_eq!(builtin.completeness(), &Completeness::Complete);
        assert!(matches!(
            external.completeness(),
            Completeness::Partial { .. }
        ));
        assert_diagnostic(&external, "PARC-P0001");
        assert!(external.macros().is_empty());
        assert!(!builtin.macros().is_empty());
        let builtin_semantics = builtin
            .declarations()
            .iter()
            .map(|declaration| {
                (
                    declaration.id,
                    declaration.name.clone(),
                    location_independent_kind(&declaration.kind),
                    declaration.support.clone(),
                )
            })
            .collect::<Vec<_>>();
        let external_semantics = external
            .declarations()
            .iter()
            .map(|declaration| {
                (
                    declaration.id,
                    declaration.name.clone(),
                    location_independent_kind(&declaration.kind),
                    declaration.support.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(builtin_semantics, external_semantics, "{name} semantics");
        assert!(builtin.declarations().iter().all(|declaration| {
            declaration
                .occurrences
                .iter()
                .all(|occurrence| occurrence.provenance.origin != SourceOrigin::Generated)
        }));
        assert!(external.declarations().iter().all(|declaration| {
            declaration
                .occurrences
                .iter()
                .all(|occurrence| occurrence.provenance.origin == SourceOrigin::Generated)
        }));
    }
}

#[cfg(feature = "system-tests")]
fn location_independent_kind(kind: &SourceDeclarationKind) -> SourceDeclarationKind {
    let mut kind = kind.clone();
    let file = FileId::from_logical_path("comparison/location").expect("comparison file id");
    let range = SourceRange {
        file,
        start: 0,
        end: 0,
    };
    let provenance = SourceProvenance {
        origin: SourceOrigin::Generated,
        include_chain: Vec::new(),
        macro_expansions: Vec::new(),
    };
    let normalize_attributes = |attributes: &mut [SourceAttribute]| {
        for attribute in attributes {
            attribute.range = range;
        }
    };
    match &mut kind {
        SourceDeclarationKind::Function(function) => {
            for parameter in &mut function.parameters {
                parameter.range = range;
                parameter.provenance = provenance.clone();
                normalize_attributes(&mut parameter.attributes);
            }
        }
        SourceDeclarationKind::Record(record) => {
            for field in &mut record.fields {
                field.range = range;
                field.provenance = provenance.clone();
                normalize_attributes(&mut field.attributes);
            }
        }
        SourceDeclarationKind::Enum(enumeration) => {
            for variant in &mut enumeration.variants {
                variant.range = range;
                variant.provenance = provenance.clone();
                normalize_attributes(&mut variant.attributes);
            }
        }
        SourceDeclarationKind::TypeAlias(_)
        | SourceDeclarationKind::Variable(_)
        | SourceDeclarationKind::Unsupported(_) => {}
    }
    kind
}

fn assert_support_diagnostic<'a>(
    package: &'a SourcePackage,
    declaration: &SourceDeclaration,
    expected_code: &str,
    expected_impact: DiagnosticCompletenessImpact,
) -> &'a SourceDiagnostic {
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
    if expected_code != "PARC-E1216" {
        assert_eq!(
            diagnostic.range,
            declaration
                .occurrences
                .last()
                .map(|occurrence| occurrence.range)
        );
    }
    diagnostic
}

fn assert_diagnostic(package: &SourcePackage, expected_code: &str) {
    let diagnostic = package
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code.as_str() == expected_code)
        .unwrap_or_else(|| {
            panic!(
                "missing diagnostic {expected_code}: {:#?}",
                package.diagnostics()
            )
        });
    if diagnostic.completeness_impact != DiagnosticCompletenessImpact::Informational {
        let reasons = match package.completeness() {
            Completeness::Complete => &[][..],
            Completeness::Partial { reasons } | Completeness::Rejected { reasons } => {
                reasons.as_slice()
            }
        };
        assert!(reasons.iter().any(|reason| {
            reason.code == diagnostic.code
                && reason.message == diagnostic.message
                && reason.range == diagnostic.range
        }));
    }
}

fn mapped_file_id_for_test(path: &str) -> FileId {
    FileId::from_logical_path(path).expect("canonical test logical path")
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .expect("script metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("make script executable");
}

#[cfg(unix)]
fn external_script_config(fixture: &Fixture, script: &std::path::Path) -> ScanConfig {
    let bytes = std::fs::read(script).expect("external script bytes");
    let compiler = CompilerIdentity::try_new(
        CompilerFamily::Gcc,
        "toolchains/test/bin/cc",
        ContentFingerprint::from_content(&bytes),
        ContentFingerprint::from_content(b"test-tool-version"),
        "x86_64-unknown-linux-gnu",
        "test-1",
    )
    .expect("test external compiler identity");
    let mapping = PathMapping::try_new([
        PathMappingRule::try_new(&fixture.root, "fixture").expect("external mapping rule")
    ])
    .expect("external mapping");
    ScanConfig::new(
        target_with_compiler("x86_64-unknown-linux-gnu", compiler),
        mapping,
        PreprocessorMode::External {
            executable: script.to_owned(),
        },
    )
    .expect("external script config")
    .entry_header(&fixture.header)
}

#[cfg(feature = "system-tests")]
fn find_system_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|value| {
        std::env::split_paths(&value)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(feature = "system-tests")]
fn system_compiler_target(executable: &std::path::Path, family: CompilerFamily) -> TargetSpec {
    let target_output = std::process::Command::new(executable)
        .arg("-dumpmachine")
        .output()
        .expect("query compiler target");
    assert!(target_output.status.success(), "compiler -dumpmachine");
    let reported_triple = String::from_utf8(target_output.stdout)
        .expect("compiler target UTF-8")
        .trim()
        .to_owned();
    let requested_triple = certified_requested_target(&reported_triple).unwrap_or_else(|| {
        panic!("certified differential target is x86_64 Linux GNU, found {reported_triple}")
    });
    let version_output = std::process::Command::new(executable)
        .arg("--version")
        .output()
        .expect("query compiler version");
    assert!(version_output.status.success(), "compiler --version");
    let file = std::fs::File::open(executable).expect("compiler executable");
    let length = file.metadata().expect("compiler metadata").len();
    let executable_fingerprint =
        ContentFingerprint::from_reader(file, length).expect("compiler fingerprint");
    let compiler = CompilerIdentity::try_new(
        family,
        format!("toolchains/{}/bin/cc", family_name(family)),
        executable_fingerprint,
        ContentFingerprint::from_content(&version_output.stdout),
        &reported_triple,
        "system",
    )
    .expect("system compiler identity");
    target_with_compiler(requested_triple, compiler)
}

#[cfg(feature = "system-tests")]
fn certified_requested_target(reported: &str) -> Option<&'static str> {
    match reported {
        "x86_64-unknown-linux-gnu" | "x86_64-linux-gnu" | "x86_64-pc-linux-gnu" => {
            Some("x86_64-unknown-linux-gnu")
        }
        _ => None,
    }
}

#[cfg(feature = "system-tests")]
fn family_name(family: CompilerFamily) -> &'static str {
    match family {
        CompilerFamily::Gcc => "gcc",
        CompilerFamily::Clang => "clang",
        CompilerFamily::AppleClang => "apple-clang",
        CompilerFamily::Msvc => "msvc",
    }
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
    let compiler = CompilerIdentity::try_new(
        CompilerFamily::Gcc,
        "toolchains/gcc/bin/gcc",
        ContentFingerprint::from_content(b"scan-test-gcc"),
        ContentFingerprint::from_content(b"scan-test-gcc-version"),
        triple,
        "13.2.0",
    )
    .expect("compiler identity");
    target_with_compiler(triple, compiler)
}

fn target_with_compiler(triple: &str, compiler: CompilerIdentity) -> TargetSpec {
    target_with_compiler_and_sysroot(triple, compiler, None)
}

fn target_with_compiler_and_sysroot(
    triple: &str,
    compiler: CompilerIdentity,
    sysroot: Option<SysrootIdentity>,
) -> TargetSpec {
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
        compiler,
        sysroot,
        abi_flags: vec![NormalizedCompilerArg::try_new("-m64").expect("ABI argument")],
    })
    .expect("target")
}
