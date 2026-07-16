# Hardening Status

This book documents the current H0 baseline. PARC is not production-certified,
the PARC/LINC/GERC pipeline is not yet certified for FOL V4, and the H1 through
H5 contracts described in the hardening plan are not implemented milestones.

## Identity And Toolchain

| Item | Current value |
|---|---|
| Distribution package | `follang-parc` |
| Rust library/import name | `parc` |
| Declared MSRV | Rust 1.89 |
| Registry publication | Deferred to the H6 distribution gate |
| Source artifact schema | Version 1; not the frozen H1 schema |

The package and library names are intentionally different. Cargo dependency
metadata uses `follang-parc`; Rust code imports `parc`.

## Current Evidence Boundary

| Surface | What exists now | What is not claimed |
|---|---|---|
| External preprocessing | GCC/Clang-driven paths and system fixtures | Hermetic availability of compilers or headers |
| Built-in preprocessing | Controlled fixture and corpus coverage | Complete C preprocessing or arbitrary system-header parity |
| `SourcePackage` | Version-1 typed fields and roundtrip tests | A fully populated artifact from every entrypoint |
| Direct extraction | Items and diagnostics | Populated target, compiler, input, macro, or provenance fields unless that entrypoint explicitly supplies them |
| Scan | Entry headers, include dirs, defines, compiler command/flavor, items, and diagnostics | A guaranteed target triple/compiler version or completeness when diagnostics report degradation |
| Platform evidence | Linux prerequisite-dependent tests; target-macro/configuration fixtures elsewhere | Certified Linux, Apple, or Windows support; native Apple/Windows CI does not exist in H0 |

PARC is a source frontend, not a name resolver, type checker, ABI/layout oracle,
or complete compiler frontend.

## Verification Interface

| Command | Purpose | Prerequisites |
|---|---|---|
| `make build` | Release build | Rust 1.89 toolchain |
| `make fmt-check` | Rust formatting check | `rustfmt` |
| `make lint` | Clippy with warnings denied | `clippy` |
| `make check-features` | Default, all-feature, and no-default checks | Cargo |
| `make test` | Hermetic required tests and doctests | Cargo; ambient C include variables are cleared |
| `make test-contract` | Source-contract tests | Cargo |
| `make test-package` | Package archive and clean-consumer check | Cargo and the repository script |
| `make test-system` | Compiler/header-dependent tests | GCC and the headers required by each selected fixture |
| `make docs-check` | mdBook and Rust API docs | `mdbook`, Cargo/rustdoc |
| `make verify` | Full non-mutating gate | All required prerequisites and a clean worktree |

Local `make test-system` defaults to `SYSTEM_TEST_MODE=optional`; unavailable
prerequisites print `SKIP`. `make verify` uses
`SYSTEM_TEST_MODE=required`, so the same absence is `FAIL`. Required CI installs
the prerequisites and must not pass through a skip. Documentation builds write
under `target/` and never stage or commit files.
