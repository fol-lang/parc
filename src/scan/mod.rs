//! Header scanning frontend for the PARC pipeline.
//!
//! This module provides `ScanConfig` and `scan_headers()` for turning
//! C headers into a `SourcePackage` — either via external preprocessing
//! (gcc/clang -E) or the built-in preprocessor.

pub mod config;

pub use config::ScanConfig;

use std::io;
use std::process::Command;

use crate::extract::Extractor;
use crate::ir::{
    DiagnosticKind, SourceDefine, SourceDiagnostic, SourceInputs, SourcePackage, SourceTarget,
};

/// Result of a header scan operation.
#[derive(Debug)]
pub struct ScanResult {
    pub package: SourcePackage,
    pub preprocessed_source: String,
}

/// Scan headers according to the given config, producing a `SourcePackage`.
pub fn scan_headers(config: &ScanConfig) -> Result<ScanResult, ScanError> {
    if config.entry_headers.is_empty() {
        return Err(ScanError::NoEntryHeaders);
    }

    let (preprocessed, compiler_cmd) = if config.use_builtin_preprocessor {
        preprocess_builtin(config)?
    } else {
        preprocess_external(config)?
    };

    let unit = match crate::parse::translation_unit(&preprocessed, config.flavor) {
        Ok(unit) => unit,
        Err(e) => {
            let mut pkg = make_base_package(config, &compiler_cmd);
            pkg.diagnostics.push(SourceDiagnostic::error(
                DiagnosticKind::ParseFailed,
                format!(
                    "parse error at line {}:{}: {:?}",
                    e.line, e.column, e.expected
                ),
            ));
            return Ok(ScanResult {
                package: pkg,
                preprocessed_source: preprocessed,
            });
        }
    };

    let extractor = Extractor::new();
    let (items, diagnostics) = extractor.extract(&unit);

    let mut pkg = make_base_package(config, &compiler_cmd);
    pkg.items = items;
    pkg.diagnostics = diagnostics;

    if config.resolve_typedefs {
        pkg.resolve_all_typedefs();
    }

    Ok(ScanResult {
        package: pkg,
        preprocessed_source: preprocessed,
    })
}

fn make_base_package(config: &ScanConfig, compiler_cmd: &str) -> SourcePackage {
    SourcePackage {
        target: SourceTarget {
            compiler_command: Some(compiler_cmd.to_string()),
            flavor: Some(format!("{:?}", config.flavor)),
            ..Default::default()
        },
        inputs: SourceInputs {
            entry_headers: config
                .entry_headers
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            include_dirs: config
                .include_dirs
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            defines: config
                .defines
                .iter()
                .map(|(k, v)| SourceDefine {
                    name: k.clone(),
                    value: v.clone(),
                })
                .collect(),
        },
        ..SourcePackage::new()
    }
}

/// Collect include paths from `C_INCLUDE_PATH` and `CPATH` environment variables.
/// These are colon-separated lists of directories, honored by gcc and clang.
fn env_include_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for var in &["C_INCLUDE_PATH", "CPATH"] {
        if let Ok(val) = std::env::var(var) {
            for entry in val.split(':') {
                if !entry.is_empty() {
                    paths.push(std::path::PathBuf::from(entry));
                }
            }
        }
    }
    paths
}

fn preprocess_external(config: &ScanConfig) -> Result<(String, String), ScanError> {
    let compiler = config.compiler.as_deref().unwrap_or(match config.flavor {
        crate::driver::Flavor::ClangC11 => "clang",
        _ => "gcc",
    });

    let mut cmd = Command::new(compiler);
    cmd.arg("-E");

    for dir in &config.include_dirs {
        cmd.arg("-I").arg(dir);
    }
    for dir in &config.system_include_dirs {
        cmd.arg("-isystem").arg(dir);
    }
    for (name, value) in &config.defines {
        match value {
            Some(v) => cmd.arg(format!("-D{}={}", name, v)),
            None => cmd.arg(format!("-D{}", name)),
        };
    }
    for header in &config.entry_headers {
        cmd.arg(header);
    }

    let output = cmd.output().map_err(ScanError::PreprocessorFailed)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(ScanError::PreprocessorError(stderr));
    }

    let source = String::from_utf8_lossy(&output.stdout).to_string();
    Ok((source, compiler.to_string()))
}

fn preprocess_builtin(config: &ScanConfig) -> Result<(String, String), ScanError> {
    use crate::preprocess::{
        builtin_headers, define_target_macros, IncludeResolver, MacroDef, MacroTable, Processor,
        Target, Token, TokenKind,
    };

    // Initialize with target macros for the host platform
    let target = Target::host();
    let mut table = MacroTable::new();
    define_target_macros(&mut table, &target);

    let mut processor = Processor::with_macros(table);
    let mut resolver = IncludeResolver::new();

    // Register built-in baseline headers before searching the filesystem.
    resolver.register_builtin_headers(builtin_headers());

    // Add user include dirs (searchable for both "..." and <...> includes)
    for dir in &config.include_dirs {
        resolver.add_local_path(dir);
        resolver.add_system_path(dir);
    }

    // Add system include dirs (for <...> includes like /usr/include)
    for dir in &config.system_include_dirs {
        resolver.add_system_path(dir);
        resolver.add_local_path(dir);
    }

    // Honor C_INCLUDE_PATH and CPATH environment variables (same as gcc/clang)
    for dir in env_include_paths() {
        resolver.add_system_path(&dir);
        resolver.add_local_path(&dir);
    }

    // Add defines
    for (name, value) in &config.defines {
        let body_text = value.as_deref().unwrap_or("1");
        let body_tokens = vec![Token {
            kind: TokenKind::Ident,
            text: body_text.to_string(),
            offset: 0,
        }];
        processor.macros_mut().define(MacroDef {
            name: name.clone(),
            params: None,
            is_variadic: false,
            body: body_tokens,
        });
    }

    // Process each entry header
    let mut all_text = String::new();
    let mut all_errors = Vec::new();

    for header in &config.entry_headers {
        let result = resolver.preprocess_file(header.as_ref(), &mut processor);
        all_text.push_str(&result.text);
        all_text.push('\n');
        all_errors.extend(result.errors);
    }

    if !all_errors.is_empty() {
        return Err(ScanError::PreprocessorError(all_errors.join("\n")));
    }

    Ok((all_text, "builtin".to_string()))
}

/// Errors from the scan operation.
#[derive(Debug)]
pub enum ScanError {
    NoEntryHeaders,
    PreprocessorFailed(io::Error),
    PreprocessorError(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::NoEntryHeaders => write!(f, "no entry headers specified"),
            ScanError::PreprocessorFailed(e) => write!(f, "preprocessor failed: {}", e),
            ScanError::PreprocessorError(s) => write!(f, "preprocessor error: {}", s),
        }
    }
}

impl std::error::Error for ScanError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_no_headers_error() {
        let config = ScanConfig::new();
        let result = scan_headers(&config);
        assert!(result.is_err());
    }

    #[test]
    fn scan_config_builder() {
        let config = ScanConfig::new()
            .entry_header("test.h")
            .include_dir("/usr/include")
            .define("VERSION", Some("2".into()))
            .define_flag("DEBUG")
            .with_flavor(crate::driver::Flavor::StdC11);

        assert_eq!(config.entry_headers.len(), 1);
        assert_eq!(config.include_dirs.len(), 1);
        assert_eq!(config.defines.len(), 2);
        assert_eq!(config.flavor, crate::driver::Flavor::StdC11);
    }

    #[test]
    fn scan_builtin_with_temp_file() {
        let dir = std::env::temp_dir().join("pac_test_scan");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("api.h"),
            "typedef int int32_t;\nint32_t get_value(void);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor();

        let result = scan_headers(&config).expect("scan should succeed");
        assert!(result.package.functions().count() >= 1);
        assert!(result.package.type_aliases().count() >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_builtin_with_include() {
        let dir = std::env::temp_dir().join("pac_test_scan_inc");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("types.h"), "typedef unsigned int uint32_t;\n").unwrap();
        std::fs::write(
            dir.join("api.h"),
            "#include \"types.h\"\nuint32_t get_id(void);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor();

        let result = scan_headers(&config).expect("scan should succeed");
        assert!(result.package.type_aliases().count() >= 1);
        assert!(result.package.functions().count() >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_config_default() {
        let config = ScanConfig::default();
        assert!(config.entry_headers.is_empty());
        assert!(!config.use_builtin_preprocessor);
    }

    #[test]
    fn scan_populates_inputs_metadata() {
        let dir = std::env::temp_dir().join("pac_test_scan_meta");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("a.h"), "int x;\n").unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("a.h"))
            .include_dir("/opt/include")
            .define("VER", Some("3".into()))
            .with_builtin_preprocessor();

        let result = scan_headers(&config).unwrap();
        assert_eq!(result.package.inputs.include_dirs.len(), 1);
        assert_eq!(result.package.inputs.defines.len(), 1);
        assert_eq!(result.package.inputs.defines[0].name, "VER");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_config_json_roundtrip() {
        let config = ScanConfig::new()
            .entry_header("api.h")
            .define_flag("NDEBUG");
        let json = serde_json::to_string(&config).unwrap();
        let back: ScanConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entry_headers.len(), 1);
        assert_eq!(back.defines.len(), 1);
    }

    #[test]
    fn scan_system_include_dir() {
        let dir = std::env::temp_dir().join("pac_test_sys_inc");
        let sysdir = dir.join("sys");
        let _ = std::fs::create_dir_all(&sysdir);

        // System header in a separate directory
        std::fs::write(sysdir.join("mytypes.h"), "typedef unsigned long size_t;\n").unwrap();
        std::fs::write(
            dir.join("api.h"),
            "#include <mytypes.h>\nsize_t get_size(void);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .system_include_dir(&sysdir)
            .with_builtin_preprocessor();

        let result = scan_headers(&config).expect("scan should succeed");
        assert!(result.package.find_type_alias("size_t").is_some());
        assert!(result.package.find_function("get_size").is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_with_resolve_typedefs() {
        let dir = std::env::temp_dir().join("pac_test_resolve");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(
            dir.join("api.h"),
            r#"
typedef unsigned int uint32_t;
typedef uint32_t handle_t;
handle_t create(void);
void destroy(handle_t h);
"#,
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor()
            .with_resolve_typedefs();

        let result = scan_headers(&config).expect("scan should succeed");
        let pkg = &result.package;

        // After resolution: handle_t's target should be UInt, not TypedefRef
        let alias = pkg.find_type_alias("handle_t").unwrap();
        assert_eq!(alias.target, crate::ir::SourceType::UInt);

        // Function return type should be resolved
        let create = pkg.find_function("create").unwrap();
        assert_eq!(create.return_type, crate::ir::SourceType::UInt);

        let destroy = pkg.find_function("destroy").unwrap();
        assert_eq!(destroy.parameters[0].ty, crate::ir::SourceType::UInt);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_system_include_with_resolve() {
        let dir = std::env::temp_dir().join("pac_test_sys_resolve");
        let sysdir = dir.join("inc");
        let _ = std::fs::create_dir_all(&sysdir);

        std::fs::write(sysdir.join("base.h"), "typedef unsigned long size_t;\n").unwrap();
        std::fs::write(
            dir.join("api.h"),
            "#include <base.h>\nvoid *alloc(size_t n);\nsize_t length(const void *p);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .system_include_dir(&sysdir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs();

        let result = scan_headers(&config).expect("scan should succeed");
        let pkg = &result.package;

        // size_t should be resolved to ULong
        let alloc = pkg.find_function("alloc").unwrap();
        assert_eq!(alloc.parameters[0].ty, crate::ir::SourceType::ULong);

        let length = pkg.find_function("length").unwrap();
        assert_eq!(length.return_type, crate::ir::SourceType::ULong);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_config_system_dirs_builder() {
        let config = ScanConfig::new()
            .entry_header("api.h")
            .system_include_dir("/usr/include")
            .system_include_dir("/usr/local/include")
            .with_resolve_typedefs();

        assert_eq!(config.system_include_dirs.len(), 2);
        assert!(config.resolve_typedefs);
    }

    #[test]
    fn scan_builtin_honors_env_include_paths() {
        // All env var tests in one function to avoid parallel race conditions
        // on C_INCLUDE_PATH / CPATH.

        let old_cinc = std::env::var("C_INCLUDE_PATH").ok();
        let old_cpath = std::env::var("CPATH").ok();

        // --- Test 1: C_INCLUDE_PATH single dir ---
        {
            let dir = std::env::temp_dir().join("pac_test_env_inc");
            let inc_dir = dir.join("envheaders");
            let _ = std::fs::create_dir_all(&inc_dir);

            std::fs::write(inc_dir.join("envtype.h"), "typedef int env_int_t;\n").unwrap();
            std::fs::write(
                dir.join("main.h"),
                "#include <envtype.h>\nenv_int_t get_env_val(void);\n",
            )
            .unwrap();

            std::env::set_var("C_INCLUDE_PATH", inc_dir.display().to_string());
            std::env::remove_var("CPATH");

            let config = ScanConfig::new()
                .entry_header(dir.join("main.h"))
                .with_builtin_preprocessor();

            let result = scan_headers(&config).expect("C_INCLUDE_PATH single dir");
            assert!(result.package.find_type_alias("env_int_t").is_some());
            assert!(result.package.find_function("get_env_val").is_some());

            let _ = std::fs::remove_dir_all(&dir);
        }

        // --- Test 2: CPATH ---
        {
            let dir = std::env::temp_dir().join("pac_test_cpath");
            let inc_dir = dir.join("cpathheaders");
            let _ = std::fs::create_dir_all(&inc_dir);

            std::fs::write(inc_dir.join("cptype.h"), "typedef long cp_long_t;\n").unwrap();
            std::fs::write(
                dir.join("main.h"),
                "#include <cptype.h>\ncp_long_t get_cp_val(void);\n",
            )
            .unwrap();

            std::env::remove_var("C_INCLUDE_PATH");
            std::env::set_var("CPATH", inc_dir.display().to_string());

            let config = ScanConfig::new()
                .entry_header(dir.join("main.h"))
                .with_builtin_preprocessor();

            let result = scan_headers(&config).expect("CPATH single dir");
            assert!(result.package.find_type_alias("cp_long_t").is_some());
            assert!(result.package.find_function("get_cp_val").is_some());

            let _ = std::fs::remove_dir_all(&dir);
        }

        // --- Test 3: C_INCLUDE_PATH with multiple colon-separated dirs ---
        {
            let dir = std::env::temp_dir().join("pac_test_env_multi");
            let inc1 = dir.join("inc1");
            let inc2 = dir.join("inc2");
            let _ = std::fs::create_dir_all(&inc1);
            let _ = std::fs::create_dir_all(&inc2);

            std::fs::write(inc1.join("a.h"), "typedef int a_t;\n").unwrap();
            std::fs::write(inc2.join("b.h"), "typedef long b_t;\n").unwrap();
            std::fs::write(
                dir.join("main.h"),
                "#include <a.h>\n#include <b.h>\na_t fa(void);\nb_t fb(void);\n",
            )
            .unwrap();

            std::env::set_var(
                "C_INCLUDE_PATH",
                format!("{}:{}", inc1.display(), inc2.display()),
            );
            std::env::remove_var("CPATH");

            let config = ScanConfig::new()
                .entry_header(dir.join("main.h"))
                .with_builtin_preprocessor();

            let result = scan_headers(&config).expect("C_INCLUDE_PATH multi dirs");
            assert!(result.package.find_function("fa").is_some());
            assert!(result.package.find_function("fb").is_some());

            let _ = std::fs::remove_dir_all(&dir);
        }

        // Restore original env
        match old_cinc {
            Some(v) => std::env::set_var("C_INCLUDE_PATH", v),
            None => std::env::remove_var("C_INCLUDE_PATH"),
        }
        match old_cpath {
            Some(v) => std::env::set_var("CPATH", v),
            None => std::env::remove_var("CPATH"),
        }
    }

    #[test]
    fn scan_builtin_stdint_h() {
        let dir = std::env::temp_dir().join("pac_test_stdint");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("api.h"),
            r#"
#include <stdint.h>
#include <stddef.h>
int32_t get_id(void);
uint64_t get_timestamp(void);
void set_data(const uint8_t *buf, size_t len);
intptr_t get_handle(void);
"#,
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor()
            .with_resolve_typedefs();

        let result = scan_headers(&config).expect("scan with stdint.h should succeed");
        let pkg = &result.package;

        // Should find all four functions
        assert!(pkg.find_function("get_id").is_some(), "missing get_id");
        assert!(
            pkg.find_function("get_timestamp").is_some(),
            "missing get_timestamp"
        );
        assert!(pkg.find_function("set_data").is_some(), "missing set_data");
        assert!(
            pkg.find_function("get_handle").is_some(),
            "missing get_handle"
        );

        // With resolve_typedefs, int32_t -> signed int, uint64_t -> unsigned long (on 64-bit)
        let get_id = pkg.find_function("get_id").unwrap();
        assert_eq!(get_id.return_type, crate::ir::SourceType::Int);

        // stdint types should be present as type aliases
        assert!(pkg.find_type_alias("int32_t").is_some());
        assert!(pkg.find_type_alias("uint64_t").is_some());
        assert!(pkg.find_type_alias("uint8_t").is_some());
        assert!(pkg.find_type_alias("intptr_t").is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_builtin_stddef_h() {
        let dir = std::env::temp_dir().join("pac_test_stddef");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("api.h"),
            "#include <stddef.h>\nvoid *alloc(size_t n);\nptrdiff_t diff(const void *a, const void *b);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor();

        let result = scan_headers(&config).expect("scan with stddef.h should succeed");
        let pkg = &result.package;

        assert!(pkg.find_function("alloc").is_some(), "missing alloc");
        assert!(pkg.find_function("diff").is_some(), "missing diff");
        assert!(pkg.find_type_alias("size_t").is_some());
        assert!(pkg.find_type_alias("ptrdiff_t").is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_builtin_stdarg_and_unistd_baseline() {
        let dir = std::env::temp_dir().join("parc_scan_builtin_stdarg_unistd");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("api.h"),
            "#include <stdarg.h>\n#include <sys/types.h>\n#include <unistd.h>\ntypedef off_t stream_off_t;\nint seek_to(stream_off_t offset, int whence);\nint format_line(const char *fmt, va_list ap);\n",
        )
        .unwrap();

        let config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor();

        let result = scan_headers(&config).expect("scan with stdarg/sys/types/unistd succeeds");
        let pkg = &result.package;

        assert!(pkg.find_type_alias("off_t").is_some());
        assert!(pkg.find_type_alias("ssize_t").is_some());
        assert!(pkg.find_type_alias("stream_off_t").is_some());
        assert!(pkg.find_function("seek_to").is_some());
        assert!(pkg.find_function("format_line").is_some());
        assert!(result.preprocessed_source.contains("va_list"));
        assert!(result.preprocessed_source.contains("stream_off_t"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "system-tests")]
    #[test]
    fn scan_builtin_vs_gcc_stdint_types() {
        const TEST_NAME: &str = "scan_builtin_vs_gcc_stdint_types";
        if !crate::tests::system_support::begin_system_test(
            TEST_NAME,
            crate::tests::system_support::command_available("gcc"),
            "gcc",
        ) {
            return;
        }

        // Compare our builtin stdint.h extraction with gcc -E extraction
        // for the same source. Both should produce equivalent resolved types.
        let dir = std::env::temp_dir().join("pac_test_stdint_cmp");
        let _ = std::fs::create_dir_all(&dir);
        let header = r#"
#include <stdint.h>
int32_t get_id(void);
uint8_t get_byte(void);
int64_t get_big(void);
"#;
        std::fs::write(dir.join("api.h"), header).unwrap();

        // Builtin preprocessor with typedef resolution
        // Keep the builtin half independent from the native toolchain's Nix/
        // system include roots; restore them before invoking gcc below.
        let saved_cpath = std::env::var_os("CPATH");
        let saved_c_include_path = std::env::var_os("C_INCLUDE_PATH");
        std::env::remove_var("CPATH");
        std::env::remove_var("C_INCLUDE_PATH");
        let builtin_config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_builtin_preprocessor()
            .with_resolve_typedefs();
        let builtin_result = scan_headers(&builtin_config).expect("builtin scan");
        match saved_cpath {
            Some(value) => std::env::set_var("CPATH", value),
            None => std::env::remove_var("CPATH"),
        }
        match saved_c_include_path {
            Some(value) => std::env::set_var("C_INCLUDE_PATH", value),
            None => std::env::remove_var("C_INCLUDE_PATH"),
        }

        let builtin_pkg = &builtin_result.package;
        let get_id = builtin_pkg.find_function("get_id").unwrap();
        let get_byte = builtin_pkg.find_function("get_byte").unwrap();
        let get_big = builtin_pkg.find_function("get_big").unwrap();

        // After resolution: int32_t -> Int, uint8_t -> UChar, int64_t -> Long (on 64-bit)
        assert_eq!(get_id.return_type, crate::ir::SourceType::Int);
        assert_eq!(get_byte.return_type, crate::ir::SourceType::UChar);
        assert_eq!(get_big.return_type, crate::ir::SourceType::Long);

        // External preprocessor (gcc) with typedef resolution
        let gcc_config = ScanConfig::new()
            .entry_header(dir.join("api.h"))
            .with_resolve_typedefs();

        let gcc_result = scan_headers(&gcc_config).expect("external gcc scan");
        let gcc_pkg = &gcc_result.package;
        let gcc_id = gcc_pkg.find_function("get_id").unwrap();
        let gcc_byte = gcc_pkg.find_function("get_byte").unwrap();
        let gcc_big = gcc_pkg.find_function("get_big").unwrap();

        // Both should resolve to the same primitive types
        assert_eq!(get_id.return_type, gcc_id.return_type, "int32_t mismatch");
        assert_eq!(
            get_byte.return_type, gcc_byte.return_type,
            "uint8_t mismatch"
        );
        assert_eq!(get_big.return_type, gcc_big.return_type, "int64_t mismatch");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
