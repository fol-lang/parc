# Readiness Scorecard

## Current posture

PARC has a checked schema-v2 contract and a deterministic, resource-bounded H2
producer for one explicitly configured translation unit. Parser/AST coverage is
broader than the certified contract-producing preprocessing subset.

Built-in scans may be complete when original files, ranges, includes, macros,
and expansions are traced and every declaration/type is modeled. External
scans remain generated-source `Partial`; `PARC-P0001` makes that provenance
boundary machine-checkable.

## Evidence by subsystem

| Subsystem | Posture |
| --- | --- |
| Contract schema, codec, IDs, and validation | Frozen schema-v2 evidence |
| Explicit scan configuration and relocation | H2 bounded deterministic evidence |
| Two-pass declaration/type lowering | Exact-type fixtures; unsupported paths remain diagnostic |
| Parser, AST traversal, and printing | Broad repository fixtures |
| Built-in preprocessing | Controlled fixtures; not universal host-header parity |
| Original include/macro provenance | Traced for the certified built-in subset |
| Complete contract macro inventory | Effective active built-in definitions; external remains unproved |
| Cross-translation-unit merge semantics | Checked target/input compatibility and stable-ID conflict rejection |
| Resource safety | Producer ceilings plus external process-group timeout/output enforcement |
| ABI layout, symbols, linking, Rust generation | Downstream ownership |

## What raises readiness

Readiness increases only when evidence closes a forcing gap, for example:

- broader certified compiler-extension and system-header coverage;
- more target families with compiler-backed parity fixtures;
- broader adversarial declaration/type preservation;
- downstream integration using only canonical serialized artifacts.

Parser success or a large fixture count alone cannot upgrade a partial scan to
complete.
