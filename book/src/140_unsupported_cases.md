# Unsupported and Partial Cases

Absence of a field is never evidence of support. Consumers must read package
`Completeness`, declaration and type `SupportStatus`, and structured
diagnostics together.

## Current closure ledger

| Family | Current state | Boundary |
| --- | --- | --- |
| K&R function declarators | Unsupported | Preserved as unsupported source structure; no guessed modern prototype |
| Block pointers | Unsupported or recovered | Not lowered as ordinary C function pointers |
| Bitfields | Source width preserved | Measured allocation/layout remains downstream |
| Known calling conventions | Modeled when unambiguous | Malformed or conflicting attributes force rejection |
| `visibility` attributes | Modeled when exact | Unknown values, malformed arguments, or conflicts force rejection |
| Other ABI-relevant attributes | Partial | Spelling/range is preserved and `PARC-P1205` forces partial completeness |
| Macro-heavy include stacks | Mode-dependent | Certified built-in constructs are traced; unsupported operators reject and external output remains provenance-partial |
| Cross-translation-unit declarations | Checked composition only | Each scan has one entry; `merge` requires target/input compatibility and rejects stable-ID conflicts |

Named parser corpora demonstrate behavior on those fixtures only. They do not
upgrade an external scan to complete provenance or establish universal
compiler/SDK compatibility.

## Semantic boundary

PARC preserves source declarations and performs conservative compatibility
checks for redeclarations. It is not a full name resolver, type checker, layout
engine, linker, or compiler-quality warning system. Record offsets, enum object
representation, symbols, and linkability belong to later evidence owners.

## Preprocessing

Scanning supports an explicit built-in mode and a fingerprint-checked external
executable. The built-in implementation is a controlled compatibility surface,
not a complete substitute for every compiler preprocessor. The older `driver`
APIs remain syntax-oriented parsing helpers and do not create a checked source
contract.

## Macro inventory and provenance

Schema v2 has a first-class macro table and macro-provenance types. The traced
built-in `scan_headers` producer records the final effective macro definitions
and original definition/invocation ranges. It rejects unsupported `#`/`##` and
ambiguous redefinitions rather than fabricating values. External preprocessing
cannot prove the same source history, so `PARC-P0001` records that gap and
forces `Completeness::Partial`.

Driver utilities may inspect active preprocessing macros, but their output is
not a substitute for checked `SourcePackage` macro evidence.

## Extensions

GNU and Clang parser profiles cover maintained syntax families, not every
version-specific extension. Unknown ABI-relevant declaration attributes are
retained with ranges and partial support; constructs that cannot be represented
without guessing are rejected or recovered with diagnostics.

## Diagnostics

Schema-v2 packages contain stable categorized `SourceDiagnostic` values with
stage, severity, completeness impact, range, target, and optional declaration
ownership. They do not currently promise compiler-style fix-its or exhaustive
warning policy. Parser recovery and unsupported lowering are explicit trust
outcomes, not silent success.

## Consumer rule

Use `ScanReport::into_complete` or `SourcePackage::into_complete` with an
explicit `Selection`. Do not special-case away forcing diagnostics, infer
completeness from parser success, or concatenate independent entry headers into
a single translation unit. Use checked package `merge` only when its target and
source-input compatibility checks succeed.
