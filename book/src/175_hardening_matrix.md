# Hardening Matrix

Confidence is separated by evidence type. A parser fixture cannot prove a
checked source contract, and a generated-source scan cannot prove original
macro provenance.

## Required hermetic evidence

| Surface | Evidence |
| --- | --- |
| Frozen contract corpus | Canonical schema-v2 bytes, IDs, target/source fingerprints, decoder and validation invariants |
| Scan-contract tests | Explicit target/path/environment configuration, two-pass lowering, ranges, diagnostics, relocation, and recovery |
| Parser API and reftests | Grammar, AST, visitor, and printer behavior |
| Package test | Published archive plus a clean downstream consumer |

`make test-contract` is the focused contract lane. `make test` adds repository
fixtures and doctests. Neither may rely on ambient C include variables.

## Required system evidence

System-dependent filters must select at least one test. The Make helper fails a
zero-test filter so a rename cannot silently greenwash the lane.

Current required system evidence covers:

- the GCC enum-representation preservation corpus;
- external-fixture refresh metadata;
- the manifest-driven full-app system runner.

This evidence is compiler/fixture specific. It is not a claim of arbitrary
OpenSSL, libcurl, libc, SDK, or host-header support.

## Conservative-failure evidence

Hardening tests also prove that PARC reports uncertainty:

- generated provenance forces `PARC-P0001` and partial completeness;
- structured parser recovery retains skipped ranges;
- malformed visibility and conflicting calling conventions reject;
- unknown ABI-relevant attributes preserve spelling and force partial;
- invalid or unrepresentable declarations are not assigned guessed IDs,
  values, or placeholder spellings.

## Interpretation

A green H1 matrix means the schema, checked construction, deterministic
single-translation-unit scan, and stated parser fixtures hold. It does not mean
complete preprocessing, macro inventory, layout proof, or whole-toolchain
production certification.
