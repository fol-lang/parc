use crate::driver::{self, Config, Flavor};
use crate::parse;

#[test]
fn parse_api_constant_works() {
    let constant = parse::constant("42", Flavor::StdC11).expect("parsing constant");
    match constant {
        crate::ast::Constant::Integer(_) => {}
        other => panic!("expected integer constant, got {:?}", other),
    }
}

#[test]
fn parse_api_expression_works() {
    parse::expression("value + 1", Flavor::StdC11).expect("parsing expression");
}

#[test]
fn parse_api_declaration_works() {
    parse::declaration("int value;", Flavor::StdC11).expect("parsing declaration");
}

#[test]
fn parse_api_statement_works() {
    parse::statement("return 0;", Flavor::StdC11).expect("parsing statement");
}

#[test]
fn parse_api_translation_unit_works() {
    parse::translation_unit(
        "typedef int counter_t;\nint main(void) { return 0; }\n",
        Flavor::StdC11,
    )
    .expect("parsing translation unit");
}

#[test]
fn parse_api_resilient_recovers_from_invalid_declaration() {
    let input = "int before;\n@@@invalid@@@;\nint after;\n";
    // Strict parse should fail
    assert!(parse::translation_unit(input, Flavor::StdC11).is_err());
    // Resilient parse should recover
    let recovered = parse::translation_unit_resilient(input, Flavor::StdC11);
    assert_eq!(
        recovered.unit.0.len(),
        2,
        "expected 2 declarations, got {}",
        recovered.unit.0.len()
    );
    assert_eq!(recovered.errors.len(), 1);
    let skipped = recovered.errors[0].skipped;
    assert_eq!(&input[skipped.start..skipped.end], "@@@invalid@@@;");
    assert!(recovered.errors[0].error.offset >= skipped.start);
    assert!(recovered.errors[0].error.offset <= skipped.end);
}

#[test]
fn parse_api_resilient_returns_all_when_valid() {
    let input = "int x;\nint y;\nint z;\n";
    let recovered = parse::translation_unit_resilient(input, Flavor::StdC11);
    assert_eq!(
        recovered.unit.0.len(),
        3,
        "expected 3 declarations, got {}",
        recovered.unit.0.len()
    );
    assert!(recovered.errors.is_empty());
}

#[test]
fn parse_builtin_end_to_end() {
    // Write a self-contained C file to a temp dir
    let dir = std::env::temp_dir().join("pac_test_builtin_e2e");
    let _ = std::fs::create_dir_all(&dir);

    std::fs::write(
        dir.join("test.c"),
        "\
#define SIZE 10
#define MAX(a, b) ((a) > (b) ? (a) : (b))

typedef unsigned long size_t;

struct point {
    int x;
    int y;
};

int arr[SIZE];

int main(void) {
    struct point p;
    p.x = MAX(3, 5);
    return 0;
}
",
    )
    .unwrap();

    let config = Config::with_gcc();
    let result = driver::parse_builtin(&config, dir.join("test.c"), &[]);
    let _ = std::fs::remove_dir_all(&dir);

    let parsed = result.expect("parse_builtin should succeed");
    assert!(
        parsed.unit.0.len() >= 3,
        "expected at least 3 top-level items (typedef, struct decl, arr, main), got {}",
        parsed.unit.0.len()
    );
}

#[test]
fn parse_builtin_with_includes() {
    let dir = std::env::temp_dir().join("pac_test_builtin_inc");
    let _ = std::fs::create_dir_all(&dir);

    std::fs::write(
        dir.join("types.h"),
        "\
#pragma once
typedef unsigned int uint32_t;
typedef unsigned long uint64_t;
",
    )
    .unwrap();

    std::fs::write(
        dir.join("main.c"),
        "\
#include \"types.h\"

uint32_t get_value(void) {
    return 42;
}
",
    )
    .unwrap();

    let config = Config::with_gcc();
    let result = driver::parse_builtin(&config, dir.join("main.c"), &[]);
    let _ = std::fs::remove_dir_all(&dir);

    let parsed = result.expect("parse_builtin with includes should succeed");
    assert!(
        parsed.unit.0.len() >= 2,
        "expected at least 2 top-level items (typedefs + function), got {}",
        parsed.unit.0.len()
    );
}
