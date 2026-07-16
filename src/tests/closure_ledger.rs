use crate::driver::Flavor;
use crate::extract::{extract_from_source, parse_and_extract};
use crate::ir::DiagnosticKind;

#[test]
fn closure_ledger_knr_function_declarations_stay_unsupported() {
    let pkg = parse_and_extract(
        "int legacy(a, b) int a; int b; { return a + b; }",
        Flavor::GnuC11,
    )
    .expect("K&R declaration should still parse");

    assert!(pkg.find_function("legacy").is_some());
    assert!(pkg.diagnostics.iter().any(|diag| {
        diag.kind == DiagnosticKind::DeclarationUnsupported
            && diag.message.contains("K&R function declaration")
            && diag.item_name.as_deref() == Some("legacy")
    }));
}

#[test]
fn closure_ledger_block_pointer_declarations_stay_unsupported() {
    let err = parse_and_extract("void (^handler)(int);", Flavor::GnuC11)
        .expect_err("block pointer declaration should remain unsupported at parse time");
    assert!(err.contains("parse error"));
    assert!(err.contains("block pointer declarators are currently unsupported"));
}

#[test]
fn closure_ledger_bitfield_records_stay_partially_represented() {
    let pkg =
        extract_from_source("struct flags { unsigned ready:1; unsigned mode:3; int plain; };")
            .expect("bitfield record should still parse");

    let record = pkg.find_record("flags").expect("record flags should exist");
    let fields = record.fields.as_ref().expect("record should have fields");
    assert_eq!(fields.len(), 3);
    assert!(fields[0].is_bitfield());
    assert!(fields[1].is_bitfield());
    assert!(!fields[2].is_bitfield());
    assert!(pkg.diagnostics.iter().any(|diag| {
        diag.kind == DiagnosticKind::DeclarationPartial
            && diag
                .message
                .contains("bitfield layout partially represented")
            && diag.item_name.as_deref() == Some("ready")
    }));
}

#[test]
fn closure_ledger_vendor_attributes_stay_partial_not_silent() {
    let pkg = parse_and_extract(
        "int api(void) __attribute__((nonnull, unused));",
        Flavor::GnuC11,
    )
    .expect("attribute declaration should still parse");

    assert!(pkg.find_function("api").is_some());
    assert!(pkg.diagnostics.iter().any(|diag| {
        diag.kind == DiagnosticKind::DeclarationPartial
            && diag.message.contains("attributes ignored")
            && diag.message.contains("nonnull")
            && diag.message.contains("unused")
            && diag.item_name.as_deref() == Some("api")
    }));
}
