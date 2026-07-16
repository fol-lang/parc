# Header Scanning

`parc::scan` is the highest-level PARC API for people who want the source
contract, not just the AST. It preprocesses headers, parses them, extracts
items, and returns a `SourcePackage` plus the preprocessed source text.

## Quick Start

```rust
use parc::scan::{ScanConfig, scan_headers};

let config = ScanConfig::new()
    .entry_header("api.h")
    .include_dir("/usr/include")
    .define_flag("NDEBUG")
    .with_builtin_preprocessor();

let result = scan_headers(&config).unwrap();
let pkg = result.package;
```

## What `scan` really owns

The scan path currently owns all of these steps:

1. choose builtin or external preprocessing
2. build the preprocessing environment
3. parse the preprocessed translation unit
4. extract declarations into `parc::ir`
5. attach input metadata and diagnostics
6. optionally resolve typedef chains in the produced package

That makes it the closest thing PARC has to a “source artifact producer”.

## ScanConfig

Builder for scan configuration:

| Method | Description |
|---|---|
| `entry_header(path)` | Add an entry-point header |
| `include_dir(path)` | Add a preprocessor include search path |
| `define(name, value)` | Add a preprocessor define with value |
| `define_flag(name)` | Add a flag-style define (no value) |
| `with_compiler(cmd)` | Set the external preprocessor command |
| `with_flavor(flavor)` | Set the parser flavor |
| `with_builtin_preprocessor()` | Use the built-in preprocessor |

## Preprocessing Modes

### External (default)

Uses `gcc -E` or `clang -E` to preprocess headers. Requires the
compiler and requested development headers to be installed. PARC delegates
preprocessing to that tool; this does not guarantee that its parser/extractor
can represent every declaration in every system header.

### Built-in

Uses `parc::preprocess` directly. This is useful for controlled fixtures and
repo-local tests. It is not a promise that the built-in preprocessor already
matches every hostile system-header stack.

## ScanResult

The scan produces:

- `package: SourcePackage` — the extracted declarations and metadata
- `preprocessed_source: String` — the preprocessed source text

The package is not implicitly complete. Scan records the entry headers,
include directories, defines, compiler command, and flavor that it observes;
target triple and compiler version may still be absent, and diagnostics can
record partial or unsupported declarations.

## Intake

For already-preprocessed source (e.g., output of `gcc -E`), use
`parc::intake::PreprocessedInput`:

```rust
use parc::intake::PreprocessedInput;

let input = PreprocessedInput::from_string("int foo(void);")
    .with_path("output.i")
    .with_flavor(parc::driver::Flavor::GnuC11);

let pkg = input.extract();
```

## What to expect from failures

`scan_headers()` can fail early on preprocessing setup problems, and it can
also return a package with parse diagnostics if preprocessing succeeded but the
source could not be fully parsed.

That split is intentional:

- operational setup failures are `Err(...)`
- source-level failures become `package.diagnostics` when possible
