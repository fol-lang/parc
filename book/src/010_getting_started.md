# Getting Started

This chapter is the shortest path from real source or headers to something that
PARC actually produces today: either a parsed AST or a `SourcePackage`.

Read `parc` as the source frontend of the toolchain:

- `parc` owns preprocessing, parsing, extraction, and source diagnostics
- `linc` owns link and binary evidence
- `gerc` owns Rust lowering and emitted build output

The boundary rule is strict: `parc/src/**` must not depend on `linc` or `gerc`,
and any cross-package translation belongs only in tests, examples, or external
harnesses.

## Add the crate

```toml
[dependencies]
parc = { path = "../parc" }
```

## Pick the right API first

Use `parc::driver` when you have a file on disk and want PARC to run a system
preprocessor first.

```rust
use parc::driver::{parse, Config};

fn main() -> Result<(), parc::driver::Error> {
    let config = Config::default();
    let parsed = parse(&config, "src/tests/files/minimal.c")?;

    println!("preprocessed bytes: {}", parsed.source.len());
    println!("top-level items: {}", parsed.unit.0.len());
    Ok(())
}
```

Use `parc::parse` when you already have source text in memory and want to parse
a fragment directly.

```rust
use parc::driver::Flavor;
use parc::parse;

fn main() {
    let expr = parse::expression("a + b * 2", Flavor::StdC11).unwrap();
    println!("{:#?}", expr);
}
```

## Choose a language flavor

PARC supports three parser modes:

| Flavor | Meaning |
| --- | --- |
| `StdC11` | Strict C11 |
| `GnuC11` | C11 plus GNU syntax such as `typeof`, attributes, statement expressions, and GNU asm |
| `ClangC11` | C11 plus Clang-oriented extensions such as availability attributes |

For file-based parsing, `Config::default()` selects:

- `clang -E` on macOS
- `gcc -E` on other targets

You can also select explicitly:

```rust
use parc::driver::Config;

let gnu = Config::with_gcc();
let clang = Config::with_clang();
```

## First useful parse example

This example parses a translation unit through the normal driver path:

```rust
use parc::driver::{parse, Config};

fn main() -> Result<(), parc::driver::Error> {
    let parsed = parse(&Config::default(), "src/tests/files/minimal.c")?;

    for (i, item) in parsed.unit.0.iter().enumerate() {
        println!("item #{i}: {:?}", item.node);
    }

    Ok(())
}
```

## First useful scan example

If what you really want is the checked source contract rather than a raw AST, start with
`parc::scan`:

```rust
use parc::scan::{
    scan_headers, PathMapping, PathMappingRule, PreprocessorMode, ScanConfig,
};

// `checked_target` is an application-provided, validated TargetSpec.
let rule = PathMappingRule::try_new(&workspace_root, "workspace")?;
let mapping = PathMapping::try_new([rule])?;
let config = ScanConfig::new(checked_target, mapping, PreprocessorMode::Builtin)?
    .entry_header(workspace_root.join("demo.h"));
let report = scan_headers(&config)?;

println!("declarations: {}", report.package().declarations().len());
```

There is no host-derived scan configuration: target identity, path mapping,
preprocessor mode, environment policy, and the single translation-unit entry
are explicit inputs.

## First fragment example

If you only need one declaration or statement, the direct parser API is faster to wire in:

```rust
use parc::driver::Flavor;
use parc::parse;

fn main() {
    let decl = parse::declaration("static const int answer = 42;", Flavor::StdC11).unwrap();
    let stmt = parse::statement("return answer;", Flavor::StdC11).unwrap();

    println!("{:#?}", decl);
    println!("{:#?}", stmt);
}
```

## What to read next

- [Common Workflows](./015_workflows.md) for choosing between `scan`, `driver`, `parse_preprocessed`, and `parse`
- [Driver API](./020_driver.md) for preprocessing and file-based parsing
- [Header Scanning](./028_scanning.md) for source-contract-first workflows
- [Parser API](./030_parser.md) for fragment parsing

## Architectural boundary

`parc` is the source frontend.

It owns:

- preprocessing
- parsing
- source extraction
- source diagnostics
- the checked `parc::contract::SourcePackage` artifact

It does not own:

- symbol inventory
- binary validation
- link planning
- Rust code generation

In this repository, cross-package composition should not live in `parc` library
code. `linc` and `gerc` should consume `parc` output only from tests, examples,
or external harnesses.
