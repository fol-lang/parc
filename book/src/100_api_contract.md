# API Contract

## Contract producer

`parc::scan::scan_headers` is the only public header-to-contract route. Its
`ScanConfig` requires explicit target, path mapping, preprocessor, environment,
and source inputs. It returns `ScanReport`, whose diagnostics are the same
fingerprinted diagnostics stored in `SourcePackage`.

## Artifact surface

`parc::contract` exposes checked target/source values, canonical codec
functions, selections, checked `retain`/`merge`, and completion checks.
`SourcePackage` is immutable; callers cannot supply a cached fingerprint or
deserialize the domain model directly.

## Syntax surface

Use these modules when a contract is not required:

| Module | Role |
| --- | --- |
| `driver` | external-preprocessor file parsing |
| `preprocess` | builtin preprocessing primitives |
| `parse` | strict or resilient parsing of controlled text |
| `ast` | typed syntax tree |
| `visit` | recursive traversal |
| `span`, `loc` | source positions |
| `print` | AST debugging |

The generated parser implementation, typedef environment, extractor, and
canonical wire DTOs are internal.

## Consumer rules

1. Never guess a host target or compiler identity.
2. Never concatenate unrelated entry headers into one scan translation unit;
   merge only checked packages with compatible target and source identity.
3. Check `Completeness` and declaration `SupportStatus` before consumption.
4. Treat ranges and provenance according to their recorded file and origin.
5. Decode/encode artifacts only through the contract codec.
6. Keep layout, symbol, link, and Rust-generation facts in downstream owners.
7. Treat parser recovery and unsupported syntax as structured outcomes.

Generated external-preprocessor scans are intentionally partial. Built-in scans
may be complete only when their traced, bounded preprocessing and lowering emit
no forcing gap. Callers must check the artifact rather than special-case a mode
or diagnostic away.
