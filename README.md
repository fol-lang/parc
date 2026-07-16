# PARC

PARC is the C source frontend for the FOL toolchain. It preprocesses and parses
one explicitly configured translation unit and produces the checked schema-v2
source contract consumed by later stages.

The Cargo package is `follang-parc`; Rust code imports it as `parc`. The crate
requires Rust 1.89 or newer.

## Public boundaries

- `parc::scan::{ScanConfig, scan_headers}` is the only public path that creates
  a source contract from headers.
- `parc::contract` contains immutable `TargetSpec`, `SourcePackage`, declaration
  and type values, canonical codecs, selection, and completion checks.
- `parc::driver`, `parc::parse`, `parc::ast`, and `parc::visit` remain available
  for syntax-level consumers.
- AST-to-contract lowering is internal. The old `parc::ir`, `parc::intake`, and
  direct public extraction routes do not exist.

Patching `SourcePackage` fields or deserializing its domain model directly is
not supported. Build a checked package through `scan_headers`, or decode a
canonical artifact with `contract::decode_source_package`.

## Scanning

Scanning has no host/default constructor. Callers must supply:

1. a validated `TargetSpec` built from explicit compiler and C data-model facts;
2. a canonical `PathMapping` from absolute physical roots to logical roots;
3. `PreprocessorMode::Builtin` or an absolute external executable whose content
   fingerprint matches `TargetSpec::compiler()`;
4. exactly one absolute entry header.

```rust,ignore
use parc::scan::{scan_headers, PathMapping, PathMappingRule, PreprocessorMode, ScanConfig};

let mapping = PathMapping::try_new([
    PathMappingRule::try_new("/absolute/project", "project")?,
])?;
let config = ScanConfig::new(checked_target, mapping, PreprocessorMode::Builtin)?
    .entry_header("/absolute/project/include/api.h");
let report = scan_headers(&config)?;

println!("declarations: {}", report.package().declarations().len());
println!("diagnostics: {}", report.diagnostics().len());
```

H1 records exact byte ranges in a generated preprocessed `SourceFile`. Original
include and macro-expansion provenance is not yet provable, so every current
scan is honestly `Completeness::Partial` and `ScanReport::into_complete` fails.
Multiple entry headers must be scanned independently; they are never
concatenated into one translation unit.

## Ownership

PARC owns source declarations, source types, diagnostics, provenance, target
identity, and effective source inputs. It does **not** own measured ABI layout,
symbol inspection, link planning, binary validation, or Rust generation.

## Verification

Use the repository Make targets:

```sh
make fmt-check
make lint
make check-features
make test
make test-contract
make test-package
make test-system
make docs-check
```

`make verify` runs the full non-mutating release gate and requires all system
prerequisites. See the book for the contract and parser details.

## License

Dual-licensed under Apache 2.0 or MIT.
