# Readiness Scorecard

## Current posture

PARC has a strong checked schema-v2 contract and a deterministic public H1
producer for one explicitly configured translation unit. Parser/AST coverage is
broader than contract-producing scan coverage.

Current scans are intentionally **not complete**: declarations point into a
generated preprocessed file, the macro table is empty, and original include and
macro-expansion provenance is unproven. `PARC-P0001` makes that limitation
machine-checkable.

## Evidence by subsystem

| Subsystem | Posture |
| --- | --- |
| Contract schema, codec, IDs, and validation | Frozen H1 evidence |
| Explicit scan configuration and relocation | H1 evidence |
| Two-pass declaration/type lowering | Focused H1 fixtures; unsupported paths remain diagnostic |
| Parser, AST traversal, and printing | Broad repository fixtures |
| Built-in preprocessing | Controlled fixtures; not universal host-header parity |
| Original include/macro provenance | H2 gap |
| Complete contract macro inventory | H2 gap |
| Cross-translation-unit merge semantics | Out of H1 scope |
| ABI layout, symbols, linking, Rust generation | Downstream ownership |

## What raises readiness

Readiness increases only when evidence closes a forcing gap, for example:

- content-addressed transitive include tables;
- exact macro definition and expansion provenance;
- compiler-backed parity fixtures tied to explicit target identities;
- broader adversarial declaration/type preservation;
- downstream integration using only canonical serialized artifacts.

Parser success or a large fixture count alone cannot upgrade a partial scan to
complete.
