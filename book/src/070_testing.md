# Testing

PARC tests three distinct public surfaces:

- parser and AST behavior
- the checked `SourcePackage` contract
- the explicit `scan_headers` path that produces that contract

Use the repository `Makefile`; it defines the supported validation lanes.

## Required lanes

```sh
make fmt-check
make lint
make check-features
make test
make test-contract
make test-package
make docs-check
```

`make test` enables `repo-tests`, clears ambient C include variables, and also
runs doctests. `make test-contract` is the focused, hermetic preservation lane
for the frozen contract corpus and scan lowering. `make test-package` checks the
published archive in a clean consumer.

## System-dependent lanes

```sh
make test-system SYSTEM_TEST_MODE=optional
make test-system SYSTEM_TEST_MODE=required
```

Optional mode reports missing compilers or headers as skips. Required mode
turns the same missing prerequisite into a failure. `make verify` uses required
mode and requires a clean worktree.

## Current test layout

| Path | Purpose |
| --- | --- |
| `src/contract/tests.rs` | Checked schema, canonical codec, IDs, validation, completeness, and embedded corpus |
| `src/tests/scan_contract.rs` | Explicit scan configuration, deterministic lowering, relocation, recovery, and adversarial attributes |
| `src/tests/parse_api.rs` | Public fragment and translation-unit parser APIs |
| `src/tests/reftests.rs` | Parser/printer fixtures under `test/reftests/` |
| `src/tests/full_apps.rs` | Manifest-driven parser/driver fixtures under `test/full_apps/` |
| `src/tests/external_tools.rs` | Compiler- and system-header-dependent probes |

The contract extractor is crate-private. Test it through `scan_headers` unless a
deliberately malformed AST is required to prove that an internal lowering
branch diagnoses loss rather than silently dropping a node.

## Reference-test updates

The reftest harness supports intentional printer updates:

```sh
TEST_UPDATE=1 make test
```

Review every rewritten expectation. This mode is for deliberate AST/printer
changes, not for accepting unexplained diffs.

## Adding coverage

1. Add a parser API test for a grammar regression.
2. Add a reftest when the printed AST shape is part of the behavior.
3. Add a scan-contract test when source semantics, provenance, diagnostics, or
   completeness are involved.
4. Add a full-app or system test only when a filesystem/compiler surface is
   essential.

Every `Partial` or `Unsupported` support reason must have a diagnostic with the
same code and message. Every diagnostic that forces partial or rejected
completeness must appear exactly once in the package's canonical completeness
reasons.

## Cross-crate proof

`parc` library tests do not import `linc` or `gerc`. Downstream consumption
belongs in those repositories or an external integration harness, using the
canonical serialized `SourcePackage` boundary.
