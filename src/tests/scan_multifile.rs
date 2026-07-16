//! Multi-file header scan tests.
//!
//! Tests covering realistic multi-header setups: include chains,
//! cross-file type references, conditional compilation, and define propagation.

use crate::ir::*;
use crate::scan::{scan_headers, ScanConfig};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pac_scan_{}", name));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn vendored_root(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/full_apps/external")
        .join(name)
        .join("header")
}

// --- Include chain: A includes B includes C ---

#[test]
fn scan_include_chain() {
    let dir = temp_dir("chain");

    std::fs::write(dir.join("base.h"), "typedef unsigned int uint32_t;\n").unwrap();
    std::fs::write(
        dir.join("mid.h"),
        "#include \"base.h\"\ntypedef uint32_t handle_t;\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("top.h"),
        "#include \"mid.h\"\nhandle_t create_handle(void);\n",
    )
    .unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("top.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;

    assert!(pkg.find_type_alias("uint32_t").is_some());
    assert!(pkg.find_type_alias("handle_t").is_some());
    assert!(pkg.find_function("create_handle").is_some());

    cleanup(&dir);
}

// --- Multiple entry headers ---

#[test]
fn scan_multiple_entry_headers() {
    let dir = temp_dir("multi_entry");

    std::fs::write(
        dir.join("module_a.h"),
        "int module_a_init(void);\nvoid module_a_cleanup(void);\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("module_b.h"),
        "int module_b_start(int flags);\nvoid module_b_stop(void);\n",
    )
    .unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("module_a.h"))
        .entry_header(dir.join("module_b.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;

    assert!(pkg.find_function("module_a_init").is_some());
    assert!(pkg.find_function("module_a_cleanup").is_some());
    assert!(pkg.find_function("module_b_start").is_some());
    assert!(pkg.find_function("module_b_stop").is_some());

    assert_eq!(pkg.inputs.entry_headers.len(), 2);

    cleanup(&dir);
}

// --- Cross-file struct references ---

#[test]
fn scan_cross_file_struct_ref() {
    let dir = temp_dir("cross_struct");

    std::fs::write(dir.join("types.h"), "struct point { int x; int y; };\n").unwrap();
    std::fs::write(
        dir.join("api.h"),
        "#include \"types.h\"\nvoid draw(struct point *p);\nstruct point origin(void);\n",
    )
    .unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("api.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;

    let point = pkg.find_record("point").unwrap();
    assert!(!point.is_opaque());
    assert_eq!(point.fields.as_ref().unwrap().len(), 2);

    let draw = pkg.find_function("draw").unwrap();
    assert_eq!(
        draw.parameters[0].ty,
        SourceType::ptr(SourceType::RecordRef("point".into()))
    );

    let origin = pkg.find_function("origin").unwrap();
    assert_eq!(origin.return_type, SourceType::RecordRef("point".into()));

    cleanup(&dir);
}

// --- Conditional compilation with defines ---

#[test]
fn scan_conditional_define() {
    let dir = temp_dir("cond_define");

    std::fs::write(
        dir.join("api.h"),
        r#"
#ifdef ENABLE_FEATURE
int feature_available(void);
#endif

int always_available(void);
"#,
    )
    .unwrap();

    // Without ENABLE_FEATURE
    let config = ScanConfig::new()
        .entry_header(dir.join("api.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert!(result.package.find_function("feature_available").is_none());
    assert!(result.package.find_function("always_available").is_some());

    // With ENABLE_FEATURE
    let config = ScanConfig::new()
        .entry_header(dir.join("api.h"))
        .define_flag("ENABLE_FEATURE")
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert!(result.package.find_function("feature_available").is_some());
    assert!(result.package.find_function("always_available").is_some());

    cleanup(&dir);
}

// --- Include guard pattern ---

#[test]
fn scan_include_guard_dedup() {
    let dir = temp_dir("inc_guard");

    std::fs::write(
        dir.join("common.h"),
        r#"
#ifndef COMMON_H
#define COMMON_H
typedef int status_t;
#endif
"#,
    )
    .unwrap();

    std::fs::write(
        dir.join("a.h"),
        "#include \"common.h\"\nstatus_t func_a(void);\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("b.h"),
        "#include \"common.h\"\nstatus_t func_b(void);\n",
    )
    .unwrap();

    std::fs::write(dir.join("main.h"), "#include \"a.h\"\n#include \"b.h\"\n").unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("main.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;

    // status_t should appear only once (include guard prevents double definition)
    assert_eq!(pkg.type_alias_count(), 1);
    assert!(pkg.find_function("func_a").is_some());
    assert!(pkg.find_function("func_b").is_some());

    cleanup(&dir);
}

// --- Vendored real library headers ---

#[test]
fn scan_vendored_zlib_headers_fail_conservatively() {
    let root = vendored_root("zlib");
    let include_dir = root.join("include");
    let entry = include_dir.join("zlib.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("vendored zlib scan should succeed");
    let pkg = &result.package;

    assert_eq!(pkg.item_count(), 0);
    assert!(pkg.has_diagnostics());
    assert!(pkg
        .diagnostics
        .iter()
        .any(|diag| diag.kind == DiagnosticKind::ParseFailed));
    assert!(result.preprocessed_source.contains("deflate"));
    assert!(result.preprocessed_source.contains("z_stream"));
}

#[test]
fn scan_vendored_musl_stdint_headers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/full_apps/external/musl/stdint");
    let include_dir = root.join("include");
    let entry = include_dir.join("stdint.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("vendored musl stdint scan should succeed");
    let pkg = &result.package;

    assert!(!pkg.has_diagnostics());
    assert!(pkg.type_alias_count() >= 20);
    assert!(pkg.find_type_alias("int32_t").is_some());
    assert!(pkg.find_type_alias("uint64_t").is_some());
    assert!(pkg.find_type_alias("uintptr_t").is_some());
    assert!(pkg.record_count() >= 1);
    assert!(result.preprocessed_source.contains("int32_t"));
    assert!(result.preprocessed_source.contains("uint64_t"));
}

#[test]
fn scan_vendored_libpng_headers() {
    let root = vendored_root("libpng");
    let include_dir = root.join("include");
    let entry = root.join("main.c");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("vendored libpng scan should now succeed with builtin limits");

    assert!(result.preprocessed_source.contains("png_byte"));
    assert!(result.preprocessed_source.contains("png_uint_32"));
    assert!(!result
        .preprocessed_source
        .contains("libpng requires 8-bit bytes"));
    assert!(!result
        .preprocessed_source
        .contains("libpng requires a signed 16-bit type"));
    assert!(!result
        .preprocessed_source
        .contains("libpng requires an unsigned 32-bit (or more) type"));
}

// --- Defines controlling ifdef ---

#[test]
fn scan_define_controls_ifdef() {
    let dir = temp_dir("def_val");

    std::fs::write(
        dir.join("config.h"),
        r#"
#ifdef USE_LARGE
int large_buffer(void);
#else
int small_buffer(void);
#endif
"#,
    )
    .unwrap();

    // Without USE_LARGE — should get small_buffer
    let config = ScanConfig::new()
        .entry_header(dir.join("config.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert!(result.package.find_function("small_buffer").is_some());
    assert!(result.package.find_function("large_buffer").is_none());

    // With USE_LARGE — should get large_buffer
    let config = ScanConfig::new()
        .entry_header(dir.join("config.h"))
        .define_flag("USE_LARGE")
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert!(result.package.find_function("large_buffer").is_some());
    assert!(result.package.find_function("small_buffer").is_none());

    cleanup(&dir);
}

// --- Preprocessed source is captured ---

#[test]
fn scan_captures_preprocessed_source() {
    let dir = temp_dir("preproc_cap");

    std::fs::write(dir.join("hello.h"), "void hello(void);\n").unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("hello.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert!(result.preprocessed_source.contains("hello"));

    cleanup(&dir);
}

// --- Empty header ---

#[test]
fn scan_empty_header() {
    let dir = temp_dir("empty_hdr");
    std::fs::write(dir.join("empty.h"), "").unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("empty.h"))
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    assert_eq!(result.package.item_count(), 0);

    cleanup(&dir);
}

// --- Include dir is respected ---

#[test]
fn scan_include_dir_resolution() {
    let dir = temp_dir("inc_dir");
    let subdir = dir.join("inc");
    let _ = std::fs::create_dir_all(&subdir);

    std::fs::write(subdir.join("defs.h"), "typedef long int64_t;\n").unwrap();
    std::fs::write(
        dir.join("api.h"),
        "#include \"defs.h\"\nint64_t get_time(void);\n",
    )
    .unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("api.h"))
        .include_dir(&subdir)
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;
    assert!(pkg.find_type_alias("int64_t").is_some());
    assert!(pkg.find_function("get_time").is_some());

    cleanup(&dir);
}

// --- Metadata populates correctly ---

#[test]
fn scan_metadata_correct() {
    let dir = temp_dir("metadata");
    std::fs::write(dir.join("x.h"), "int x;\n").unwrap();

    let config = ScanConfig::new()
        .entry_header(dir.join("x.h"))
        .include_dir("/some/path")
        .define("FOO", Some("bar".into()))
        .define_flag("BAZ")
        .with_builtin_preprocessor();

    let result = scan_headers(&config).unwrap();
    let pkg = &result.package;

    assert_eq!(pkg.inputs.entry_headers.len(), 1);
    assert_eq!(pkg.inputs.include_dirs.len(), 1);
    assert_eq!(pkg.inputs.defines.len(), 2);
    assert!(pkg.target.flavor.is_some());
    assert_eq!(pkg.target.compiler_command.as_deref(), Some("builtin"));

    cleanup(&dir);
}
