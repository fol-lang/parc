# Hardening Status

PARC exposes the checked schema-v2 source-package contract and the bounded H2
header-to-contract producer. This is source-frontend evidence, not arbitrary
host-header or whole-toolchain certification.

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
logical files, declarations, macros, diagnostics, completeness, provenance,
and canonical IDs. Its decoder rejects unknown schema versions and forged
cross-references. `retain` closes over declaration references; `merge` rejects
target, input, file, declaration, and macro conflicts before rebuilding a
checked fingerprint.

The built-in scan path traces original ranges, transitive include content,
include chains, effective macro definitions, and macro expansions. It may be
`Complete` only when no forcing gap remains. Unsupported directives or macro
operators, malformed conditionals, recovery, type uncertainty, and resource
ceilings are structured `Partial` or `Rejected` outcomes. External preprocessing
still carries `PARC-P0001` because generated compiler output cannot prove
original provenance. Layout, binary symbols, link facts, and Rust-generation
facts remain downstream responsibilities.

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
| `make test-contract-system` | Required GCC enum and GCC/Clang differential evidence |
| `make test-package` | Package archive and clean-consumer check |
| `make test-system` | Compiler/header-dependent test group |
| `make docs-check` | mdBook and Rust API documentation |
| `make verify` | Full clean-worktree gate |

System lanes support optional and required prerequisite modes. Release evidence
must use required mode; a skip is not a pass.
