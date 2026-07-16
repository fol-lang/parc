use crate::ir::DiagnosticKind;
use crate::scan::{scan_headers, ScanConfig};

fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "parc_failure_matrix_{tag}_{}_{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn failure_matrix_preprocess_missing_entry_header_is_hard_error() {
    let missing = unique_temp_dir("missing").join("no_such_header.h");
    let err = scan_headers(
        &ScanConfig::new()
            .entry_header(&missing)
            .with_builtin_preprocessor(),
    )
    .expect_err("missing entry header should be a hard error");

    let msg = format!("{err:?}");
    assert!(msg.contains("PreprocessorError") || msg.contains("cannot open"));
}

#[test]
fn failure_matrix_preprocess_directive_error_is_hard_error() {
    let dir = unique_temp_dir("directive");
    let header = dir.join("bad.h");
    std::fs::write(
        &header,
        "#error \"frontend failure matrix probe\"\nint never_seen(void);\n",
    )
    .unwrap();

    let err = scan_headers(
        &ScanConfig::new()
            .entry_header(&header)
            .with_builtin_preprocessor(),
    )
    .expect_err("#error should stop scan_headers");

    let msg = format!("{err:?}");
    assert!(msg.contains("frontend failure matrix probe"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failure_matrix_preprocess_vendored_zlib_stays_conservative_and_diagnostic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/full_apps/external")
        .join("zlib")
        .join("header");
    let include_dir = root.join("include");
    let entry = include_dir.join("zlib.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("vendored zlib scan should succeed conservatively");

    assert_eq!(result.package.item_count(), 0);
    assert!(result.package.has_diagnostics());
    assert!(result
        .package
        .diagnostics
        .iter()
        .any(|diag| diag.kind == DiagnosticKind::ParseFailed));
    let status = result.package.extraction_status();
    assert_eq!(status.trustworthy_item_count, 0);
    assert_eq!(status.parse_failure_count, 1);
    assert!(result.preprocessed_source.contains("z_stream"));
}
