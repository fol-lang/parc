# Hardening Status

PARC now exposes the checked H1 source-package contract and a single public
header-to-contract producer. This is a contract milestone, not a production or
whole-toolchain certification.

## Identity and schema

| Item | Current value |
| --- | --- |
| Distribution package | `follang-parc` |
| Rust library/import name | `parc` |
| Declared MSRV | Rust 1.89 |
| Source artifact schema | `follang.parc.source-package`, version 2 |
| Public producer | `parc::scan::scan_headers` |

## Evidence boundary

The checked contract preserves explicit target identity, effective inputs,
logical files, declarations, diagnostics, completeness, and canonical IDs. Its
decoder rejects unknown schema versions and forged cross-references.

Current scans deliberately stop at generated-preprocessed provenance.
`PARC-P0001` therefore forces `Completeness::Partial`; transitive include
content and macro-expansion provenance are not silently invented. Layout,
binary symbols, link facts, and Rust-generation facts remain downstream
responsibilities.

The old public `ir`, `intake`, and direct extractor routes do not exist. Parser
and AST APIs remain available for syntax-oriented consumers, but they do not
construct a source contract.

## Verification interface

| Command | Purpose |
| --- | --- |
| `make fmt-check` | Formatting gate |
| `make lint` | Clippy with warnings denied |
| `make check-features` | Default, all-feature, and no-default checks |
| `make test` | Repository tests and doctests |
| `make test-contract` | Frozen contract corpus and scan preservation |
| `make test-contract-system` | Required compiler-backed enum-representation probe |
| `make test-package` | Package archive and clean-consumer check |
| `make test-system` | Compiler/header-dependent test group |
| `make docs-check` | mdBook and Rust API documentation |
| `make verify` | Full clean-worktree gate |

System lanes support optional and required prerequisite modes. Release evidence
must use required mode; a skip is not a pass.
