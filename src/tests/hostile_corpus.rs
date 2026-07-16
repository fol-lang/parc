use std::path::Path;

use crate::scan::{scan_headers, ScanConfig};

fn corpus_root(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/corpus")
        .join(name)
}

#[cfg(feature = "system-tests")]
fn begin_external_test(name: &str) -> bool {
    super::system_support::begin_system_test(
        name,
        super::system_support::command_available("gcc"),
        "gcc",
    )
}

#[test]
fn hostile_macro_env_corpus_scans_with_builtin_preprocessor() {
    let root = corpus_root("macro_env_a");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("macro-hostile corpus should scan");

    let pkg = &result.package;
    assert!(pkg.find_type_alias("corpus_handle_t").is_some());
    assert!(pkg.find_record("corpus_config").is_some());
    assert!(pkg.find_function("corpus_open").is_some());
    assert!(pkg.find_function("corpus_format").is_some());
    assert!(result.preprocessed_source.contains("corpus_format"));
    assert!(result.preprocessed_source.contains("va_list"));
}

#[test]
fn hostile_type_env_corpus_scans_with_builtin_preprocessor() {
    let root = corpus_root("type_env_b");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor(),
    )
    .expect("type-hostile corpus should scan");

    let pkg = &result.package;
    assert!(pkg.find_type_alias("item_t").is_some());
    assert!(pkg.find_type_alias("item_iter_cb").is_some());
    assert!(pkg.find_record("item").is_some());
    assert!(pkg.find_record("item_bucket").is_some());
    assert!(pkg.find_function("iterate_bucket").is_some());
    assert!(result.preprocessed_source.contains("item_iter_cb"));
    assert!(result.preprocessed_source.contains("item_bucket"));
}

#[test]
fn hostile_combo_env_corpus_scans_with_builtin_preprocessor() {
    let root = corpus_root("combo_env_c");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("combined hostile corpus should scan");

    let pkg = &result.package;
    assert!(pkg.find_type_alias("combo_word_t").is_some());
    assert!(pkg.find_record("combo_config").is_some());
    assert!(pkg.find_enum("combo_mode").is_some());
    assert!(pkg.find_function("combo_open").is_some());
    assert!(pkg.find_function("combo_log").is_some());
    assert!(result.preprocessed_source.contains("combo_word_t"));
    assert!(result.preprocessed_source.contains("combo_log"));
}

#[test]
fn hostile_order_env_d_corpus_scans_with_builtin_preprocessor() {
    let root = corpus_root("order_env_d");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("order hostile corpus should scan");

    let pkg = &result.package;
    assert!(pkg.find_type_alias("order_word_t").is_some());
    assert!(pkg.find_record("order_packet").is_some());
    assert!(pkg.find_enum("order_mode").is_some());
    assert!(pkg.find_function("order_open").is_some());
    assert!(pkg.find_function("order_log").is_some());
    assert!(result.preprocessed_source.contains("ORDER_MODE_WIDE"));
    assert!(result.preprocessed_source.contains("order_log"));
}

#[test]
fn hostile_include_env_e_corpus_scans_with_builtin_preprocessor() {
    let root = corpus_root("include_env_e");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let result = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("include-order hostile corpus should scan");

    let pkg = &result.package;
    assert!(pkg.find_type_alias("include_counter_t").is_some());
    assert!(pkg.find_type_alias("include_log_sink").is_some());
    assert!(pkg.find_record("include_frame").is_some());
    assert!(pkg.find_function("include_open").is_some());
    assert!(pkg.find_function("include_flush").is_some());
    assert!(result.preprocessed_source.contains("include_frame"));
    assert!(result.preprocessed_source.contains("va_list"));
}

#[cfg(feature = "system-tests")]
#[test]
fn hostile_macro_env_corpus_builtin_and_external_preprocessors_agree_on_items() {
    if !begin_external_test(
        "hostile_macro_env_corpus_builtin_and_external_preprocessors_agree_on_items",
    ) {
        return;
    }

    let root = corpus_root("macro_env_a");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let builtin = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("builtin macro corpus scan should succeed");

    let external = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_resolve_typedefs(),
    )
    .expect("external macro corpus scan should succeed");

    assert!(builtin.package.find_type_alias("corpus_handle_t").is_some());
    assert!(external
        .package
        .find_type_alias("corpus_handle_t")
        .is_some());
    assert!(builtin.package.find_function("corpus_open").is_some());
    assert!(external.package.find_function("corpus_open").is_some());
    assert!(builtin.package.find_function("corpus_format").is_some());
    assert!(external.package.find_function("corpus_format").is_some());
    assert!(external.package.item_count() >= builtin.package.item_count());
    assert!(external.preprocessed_source.contains("corpus_format"));
}

#[cfg(feature = "system-tests")]
#[test]
fn hostile_combo_env_corpus_builtin_and_external_preprocessors_agree_on_items() {
    if !begin_external_test(
        "hostile_combo_env_corpus_builtin_and_external_preprocessors_agree_on_items",
    ) {
        return;
    }

    let root = corpus_root("combo_env_c");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let builtin = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("builtin combo corpus scan should succeed");

    let external = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_resolve_typedefs(),
    )
    .expect("external combo corpus scan should succeed");

    assert!(builtin.package.find_type_alias("combo_word_t").is_some());
    assert!(external.package.find_type_alias("combo_word_t").is_some());
    assert!(builtin.package.find_enum("combo_mode").is_some());
    assert!(external.package.find_enum("combo_mode").is_some());
    assert!(builtin.package.find_record("combo_config").is_some());
    assert!(external.package.find_record("combo_config").is_some());
    assert!(builtin.package.find_function("combo_open").is_some());
    assert!(external.package.find_function("combo_open").is_some());
    assert!(builtin.package.find_function("combo_log").is_some());
    assert!(external.package.find_function("combo_log").is_some());
    assert!(external.package.item_count() >= builtin.package.item_count());
    assert!(external.preprocessed_source.contains("combo_log"));
}

#[cfg(feature = "system-tests")]
#[test]
fn hostile_order_env_d_corpus_builtin_and_external_preprocessors_agree_on_items() {
    if !begin_external_test(
        "hostile_order_env_d_corpus_builtin_and_external_preprocessors_agree_on_items",
    ) {
        return;
    }

    let root = corpus_root("order_env_d");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let builtin = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("builtin order corpus scan should succeed");

    let external = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_resolve_typedefs(),
    )
    .expect("external order corpus scan should succeed");

    assert!(builtin.package.find_type_alias("order_word_t").is_some());
    assert!(external.package.find_type_alias("order_word_t").is_some());
    assert!(builtin.package.find_record("order_packet").is_some());
    assert!(external.package.find_record("order_packet").is_some());
    assert!(builtin.package.find_enum("order_mode").is_some());
    assert!(external.package.find_enum("order_mode").is_some());
    assert!(builtin.package.find_function("order_open").is_some());
    assert!(external.package.find_function("order_open").is_some());
    assert!(builtin.package.find_function("order_log").is_some());
    assert!(external.package.find_function("order_log").is_some());
    assert!(external.package.item_count() >= builtin.package.item_count());
    assert!(external.preprocessed_source.contains("order_log"));
}

#[cfg(feature = "system-tests")]
#[test]
fn hostile_include_env_e_corpus_builtin_and_external_preprocessors_agree_on_items() {
    if !begin_external_test(
        "hostile_include_env_e_corpus_builtin_and_external_preprocessors_agree_on_items",
    ) {
        return;
    }

    let root = corpus_root("include_env_e");
    let include_dir = root.join("include");
    let entry = root.join("entry.h");

    let builtin = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_builtin_preprocessor()
            .with_resolve_typedefs(),
    )
    .expect("builtin include-order corpus scan should succeed");

    let external = scan_headers(
        &ScanConfig::new()
            .entry_header(&entry)
            .include_dir(&include_dir)
            .with_resolve_typedefs(),
    )
    .expect("external include-order corpus scan should succeed");

    assert!(builtin
        .package
        .find_type_alias("include_counter_t")
        .is_some());
    assert!(external
        .package
        .find_type_alias("include_counter_t")
        .is_some());
    assert!(builtin
        .package
        .find_type_alias("include_log_sink")
        .is_some());
    assert!(external
        .package
        .find_type_alias("include_log_sink")
        .is_some());
    assert!(builtin.package.find_record("include_frame").is_some());
    assert!(external.package.find_record("include_frame").is_some());
    assert!(builtin.package.find_function("include_open").is_some());
    assert!(external.package.find_function("include_open").is_some());
    assert!(builtin.package.find_function("include_flush").is_some());
    assert!(external.package.find_function("include_flush").is_some());
    assert!(external.package.item_count() >= builtin.package.item_count());
    assert!(external.preprocessed_source.contains("include_flush"));
}
