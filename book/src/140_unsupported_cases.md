# Unsupported Cases

This chapter records the important unsupported or intentionally out-of-scope
areas.

The goal is to prevent downstream users from mistaking absence of detail for
implicit support.

It also acts as the current frontend-family closure ledger. Every hard family
should fit into one of these buckets:

- covered on named fixtures
- resilient-only behavior
- diagnostics-only improvement
- intentional rejection

This is a pre-certification evidence ledger. If a family is not classified
here, it has no documented fixture-backed behavior; classification still does
not make it a production guarantee.

## Frontend-Family Closure Ledger

The current important families are:

| Family | Current state | Notes |
| --- | --- | --- |
| K&R function declarations | diagnostics-only improvement | PARC preserves the function surface and emits explicit unsupported diagnostics. |
| block pointers | intentional rejection | They still fail in parsing; current work is about sharper diagnostics, not pretending they lower cleanly. |
| bitfield-heavy records | resilient-only support | PARC keeps record shape and bit widths, but layout truth remains partial. |
| vendor attributes and calling-convention attributes | resilient-only support | PARC preserves the declaration and emits partial diagnostics when attributes are ignored. |
| macro-heavy include stacks | covered on named repository corpora | The named corpora are only regression evidence; they do not prove broad closure. |
| hostile include-order and typedef-chain environments | covered on named repository corpora | Treat this as corpus-backed evidence, not universal extension parity. |

This ledger is intentionally blunt:

- if a family is not yet honestly representable, reject it
- if a family is only partially representable, say so
- if a family is only proven on named corpora, document that exact scope

There is no production platform envelope during H0. Read each row literally as
fixture coverage, partial behavior with diagnostics, or explicit rejection.

## Semantic analysis

PARC does not provide:

- full name resolution
- type checking
- constant folding as a stable analysis contract
- ABI or layout proof
- compiler-quality warnings

It is a parser with source-structure support, not a complete compiler frontend.

## Preprocessing

PARC does not implement a standalone C preprocessor in the `driver` path.

Instead it depends on an external preprocessor command such as:

- `gcc -E`
- `clang -E`

That means PARC does not try to normalize every compiler's preprocessing
behavior internally.

The built-in preprocessor is increasingly useful for scan-first workflows, but
it is still a scoped compatibility surface rather than a promise of universal
host-header parity.

## Extension completeness

PARC supports several GNU and Clang extensions, but the project does not promise
complete parity with every extension accepted by modern GCC or Clang releases.

Downstream tools should not assume:

- full GNU extension completeness
- full Clang extension completeness
- identical acceptance behavior across all compiler-version-specific syntax edges

## Macro inventory and expansion modeling

PARC has macro-capture helpers and `SourcePackage.macros`, but not every
entrypoint populates them and the current version-1 artifact does not promise a
complete, stable semantic inventory of macro definitions.

## Translation-unit semantics

PARC can parse translation units, but it does not guarantee:

- cross-file symbol resolution
- duplicate-definition analysis as a stable feature
- semantic correctness of declarations
- linkability of parsed declarations

Those tasks belong to later analysis layers, not the parser itself.

## Diagnostics depth

PARC does not currently provide:

- warning classes
- fix-it suggestions
- rich categorized error codes
- a stable diagnostic JSON schema

The current error model is strong enough for syntax handling, not full compiler
UX.

The practical rule for the remaining hard families is:

- if PARC can keep a trustworthy declaration surface, it should do so and emit
  diagnostics
- if PARC cannot keep a trustworthy declaration surface, it should reject the
  construct explicitly

## Consumer guidance

Downstream tools should treat these gaps as explicit non-guarantees.

That means:

- build policy around syntax success and failure, not semantic certainty
- isolate extension-heavy assumptions behind tests
- keep representative preprocessed fixtures for any hard parser dependency
- treat the closure ledger above as part of the real contract, not as a vague
  future roadmap
