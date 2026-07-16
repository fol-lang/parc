use crate::driver::Flavor;
use crate::extract;

#[test]
fn failure_matrix_source_malformed_input_is_hard_error() {
    let err = extract::extract_from_source("struct broken {")
        .expect_err("unterminated struct should be a hard parse error");
    let msg = format!("{err:?}");
    assert!(!msg.trim().is_empty());
}

#[test]
fn failure_matrix_source_resilient_mode_keeps_good_items() {
    let pkg = extract::parse_and_extract_resilient(
        "typedef int ok_t;\n@@garbage@@;\nint still_ok(void);",
        Flavor::GnuC11,
    );

    assert!(pkg.find_type_alias("ok_t").is_some());
    assert!(pkg.find_function("still_ok").is_some());
    assert!(pkg.item_count() >= 2);
    let status = pkg.extraction_status();
    assert_eq!(status.trustworthy_item_count, pkg.item_count());
    assert_eq!(status.parse_failure_count, 0);
    assert!(pkg
        .extraction_status_message()
        .contains("trustworthy extracted items"));
}

#[test]
fn failure_matrix_source_static_helper_becomes_diagnostic_not_silent_drop() {
    let pkg = extract::extract_from_source(
        r#"
        static int helper(void) { return 0; }
        int public_api(void);
        "#,
    )
    .unwrap();

    assert!(pkg.find_function("public_api").is_some());
    assert!(pkg.has_diagnostics());
    assert!(pkg
        .diagnostics
        .iter()
        .any(|diag| diag.message.contains("static")));
    let status = pkg.extraction_status();
    assert_eq!(status.trustworthy_item_count, pkg.item_count());
    assert!(status.unsupported_diagnostic_count >= 1);
}
