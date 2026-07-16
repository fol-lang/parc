# Extraction

The `parc::extract` module converts a parsed C AST into the normalized
`SourcePackage` IR. It handles the declaration families listed below and emits
diagnostics or rejects input outside the current representation boundary; it
does not claim complete C declaration coverage.

## Quick Start

```rust
use parc::extract;

let source = r#"
    typedef unsigned long size_t;
    void *malloc(size_t size);
    struct point { int x; int y; };
"#;

let pkg = extract::extract_from_source(source).unwrap();
assert_eq!(pkg.function_count(), 1);
assert_eq!(pkg.record_count(), 1);
assert_eq!(pkg.type_alias_count(), 1);
```

## API Functions

### `extract_from_source`

Parse and extract in one step using GNU C11 flavor:

```rust
let pkg = parc::extract::extract_from_source("int foo(void);").unwrap();
```

### `parse_and_extract`

Parse and extract with a specific flavor:

```rust
let pkg = parc::extract::parse_and_extract(
    "int foo(void);",
    parc::driver::Flavor::StdC11,
).unwrap();
```

### `extract_from_translation_unit`

Extract from an already-parsed AST:

```rust
let unit = parc::parse::translation_unit("int foo(void);", parc::driver::Flavor::StdC11).unwrap();
let pkg = parc::extract::extract_from_translation_unit(&unit, Some("test.h".into()));
```

### `parse_and_extract_resilient`

Parse with error recovery and extract what's possible:

```rust
let pkg = parc::extract::parse_and_extract_resilient(
    "int valid;\n@@@bad@@@;\nint also_valid;",
    parc::driver::Flavor::StdC11,
);
```

### `extract_file`

Read a file from disk and extract:

```rust
let pkg = parc::extract::extract_file("path/to/header.h", parc::driver::Flavor::GnuC11).unwrap();
assert!(pkg.source_path.is_some());
```

## What Gets Extracted

| C Declaration | Source Item |
|---|---|
| `typedef int T;` | `SourceTypeAlias` |
| `int foo(void);` | `SourceFunction` |
| `int foo(void) { ... }` | `SourceFunction` (body ignored) |
| `struct S { int x; };` | `SourceRecord` |
| `struct S;` | `SourceRecord` (opaque) |
| `union U { ... };` | `SourceRecord` (Union kind) |
| `enum E { A, B };` | `SourceEnum` |
| `extern int x;` | `SourceVariable` |
| `static int f() {}` | Diagnostic (not bindable) |
| `_Static_assert(...)` | Diagnostic |

## Diagnostics

The extractor produces diagnostics for constructs it cannot fully represent:

- Bitfield widths (partial representation)
- Inline/noreturn specifiers (ignored)
- Calling convention attributes (captured on function, other attributes warned)
- K&R function declarations (unsupported)
- Block pointers (unsupported)
- Static functions (not bindable)

Direct extraction entrypoints populate items and diagnostics but otherwise
start from `SourcePackage::new()`. Target identity, compiler metadata, inputs,
macros, and provenance can therefore remain empty/default. Use the scan path
when its observed input metadata is required, and still inspect diagnostics
before treating any artifact as complete.
