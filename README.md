# PARC

PARC is the source frontend of the toolchain. It owns preprocessing, parsing,
header scanning, source extraction, source diagnostics, and the PARC-owned
source IR.

## Hardening Status

PARC is being hardened as the source-contract owner for the sibling
PARC/LINC/GERC pipeline. It is not yet certified for FOL V4.

The distribution package is `follang-parc`; the Rust library name remains
`parc`. Registry publication is deferred until the H6 distribution gate, and
the crate version remains unchanged during baseline hardening. The declared
minimum supported Rust version (MSRV) is Rust 1.89.

## Current Support Boundary

| Area | Current evidence | Boundary |
|---|---|---|
| External preprocessing | Exercised with GCC/Clang on configured hosts | Requires the compiler and requested headers; it is not hermetic system-header support. |
| Built-in preprocessing | Exercised on repository fixtures | Incomplete; it is not a replacement for a production C preprocessor or proof of arbitrary host-header support. |
| Parsing and extraction | Covered for the documented declaration fixtures | Not a complete C semantic frontend, name resolver, type checker, or ABI/layout authority. |
| `SourcePackage` schema | Schema version 1 roundtrips are tested | A package is not necessarily complete: direct extraction leaves target/input fields at defaults, scan only populates the metadata it observes, and diagnostics can record partial or unsupported input. |
| Linux system fixtures | Prerequisite-dependent tests exist | These are system-test evidence, not a certified platform tier. |
| Apple and Windows | Target-macro and synthetic/configuration code paths exist | Neither platform is certified; there is no native Apple or Windows CI gate in H0. |

The current crate surface is broader than “just a parser”:

- `driver` parses files through an external preprocessor
- `preprocess` provides a built-in C preprocessor
- `parse` parses source fragments directly
- `extract` lowers AST into source IR
- `scan` turns real headers into a `SourcePackage`
- `ir` is the durable source contract

## Ownership

PARC owns:

- C preprocessing and preprocessing-related capture helpers
- C parsing and parser recovery
- AST traversal, spans, locations, and debug printing
- source-level extraction into `parc::ir::SourcePackage`
- end-to-end header scanning via `parc::scan`

PARC does not own:

- symbol inspection
- binary validation
- link planning
- Rust lowering or emission

## Real Public Surface

The most important public entrypoints today are:

- `parc::driver::{Config, parse, parse_preprocessed, parse_builtin, capture_macros}`
- `parc::scan::{ScanConfig, scan_headers}`
- `parc::extract::{Extractor, extract_from_source, parse_and_extract, parse_and_extract_resilient}`
- `parc::parse::*` for fragment parsing
- `parc::ir::*` for the source contract
- `parc::visit`, `parc::span`, `parc::loc`, and `parc::print`

The crate root is intentionally broad because PARC still serves both:

- downstream consumers that only want `SourcePackage`
- parser/AST-level consumers that want direct syntax access

## Fastest Working Paths

Parse a file through the normal external-preprocessor path:

```rust
use parc::driver::{parse, Config};

let parsed = parse(&Config::default(), "src/tests/files/minimal.c").unwrap();
println!("top-level items: {}", parsed.unit.0.len());
```

Scan headers and produce source IR directly:

```rust
use parc::scan::{scan_headers, ScanConfig};

let config = ScanConfig::new().entry_header("demo.h");
let result = scan_headers(&config).unwrap();
println!("ir items: {}", result.package.items.len());
```

Parse a fragment from memory:

```rust
use parc::driver::Flavor;
use parc::parse;

let expr = parse::expression("a + b * 2", Flavor::StdC11).unwrap();
println!("{expr:#?}");
```

## Artifact Boundary

`parc` owns its own source model and serialized source artifacts.

The durable boundary is `parc::ir::SourcePackage`, which contains:

- extracted items
- source types
- macros and input metadata
- provenance and diagnostics
- partial/unsupported source results

Cross-package translation still belongs outside `parc/src/**`. PARC can be
used in integration tests and harnesses, but its library code is not where
downstream link or generation wiring should live.

## Tested Scope

The current suite covers:

- parser and preprocessor behavior
- scan/extract/source-contract behavior
- determinism and JSON/source-artifact roundtrips
- hostile headers, system headers, and full-app fixtures
- explicit preprocess and source failure matrices
- external-fixture corpora under `src/tests/**`

The tests are the best statement of what PARC actually supports.

## Current Contract

For the current hardening baseline, the PARC contract is:

- supported families are supported on the named canonical corpus
- partial families emit explicit partial diagnostics
- diagnostics-only families preserve useful source surface where possible
- rejected families fail explicitly rather than degrading silently

This is a bounded, pre-certification frontend contract, not a universal C
compatibility or production-readiness claim.

## Current Test Evidence

The current hardening ladder is easiest to read in four buckets:

- hermetic vendored baselines
  - musl `stdint`
  - vendored zlib
  - vendored libpng builtin-preprocessor success surface
- host-dependent public-header ladders
  - OpenSSL public wrapper extraction
  - libcurl public wrapper extraction
  - Linux combined event-loop wrapper extraction
- hostile and degraded surfaces
  - hostile declaration fixtures
  - repo-owned `macro_env_a` hostile macro corpus
  - repo-owned `type_env_b` hostile type corpus
  - explicit unsupported-family closure ledger for K&R, block pointers,
    bitfield-heavy records, and vendor attributes
  - resilient recovery fixtures
  - explicit preprocess failure matrix
  - explicit source refusal and recovery matrix
  - extraction-status summaries that distinguish supported, partial, and unsupported surfaces
- determinism anchors
  - vendored musl scan
  - vendored zlib scan
  - vendored libpng scan
  - `macro_env_a` scan
  - `type_env_b` scan
  - OpenSSL wrapper extraction
  - libcurl wrapper extraction
  - combined Linux event-loop wrapper extraction

Read those as the current confidence anchors, not as a promise that every
system header family is equally mature.

The current frontend evidence surfaces include:

- vendored musl `stdint`
- vendored zlib scan
- vendored libpng scan
- repo-owned `macro_env_a` hostile macro corpus
- repo-owned `type_env_b` hostile type corpus
- OpenSSL public wrapper extraction
- libcurl public wrapper extraction
- combined Linux event-loop wrapper extraction

The current PARC test corpus is intentionally named:

- hermetic vendored
  - musl `stdint`
  - zlib public headers
  - libpng public headers
- hermetic synthetic hostile
  - `test/corpus/macro_env_a`
  - `test/corpus/type_env_b`
- host-dependent raises
  - OpenSSL public wrapper extraction
  - libcurl public wrapper extraction
  - combined Linux event-loop wrapper extraction
- conservative-failure anchors
  - vendored zlib builtin-preprocessor conservative parse failure
  - malformed-source hard errors
  - resilient-source recovery paths

Those are test anchors, not certification. H1 through H5 of the hardening plan
remain future milestones; no corpus result by itself proves the cross-repository
contract or a production platform.

## Verification

```sh
make build
make fmt-check
make lint
make check-features
make test
make test-contract
make test-package
make test-system
make docs-check
make verify
```

`make test` is the hermetic required lane. `make test-system` defaults to
`SYSTEM_TEST_MODE=optional`: each unavailable compiler/header prerequisite is
reported as `SKIP`. `make verify` reruns that lane with
`SYSTEM_TEST_MODE=required`, so a missing prerequisite is `FAIL`; required CI
uses the same behavior. `make docs-check` requires `mdbook` and builds both the
book and Rust API documentation without staging or committing output.

`make verify` expects a clean worktree and confirms that the gate did not
change it. During local review of an already-dirty tree,
`VERIFY_ALLOW_DIRTY=1 make verify` retains the before/after cleanliness check.

## Development Notes

The parser implementation lives under `src/parser/`.

The main source-contract and integration fixtures live under:

- `src/tests/`
- `src/tests/full_apps.rs`
- `src/tests/system_headers.rs`
- `src/tests/hostile_headers.rs`

The book is intentionally more detailed than this README. Start there if you
need the exact contract story for `driver`, `scan`, `extract`, or `ir`.

## License

Dual-licensed under Apache 2.0 or MIT (see `LICENSE-APACHE` and
`LICENSE-MIT`).
