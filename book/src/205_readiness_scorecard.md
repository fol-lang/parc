# Hardening Evidence Scorecard

This chapter ties PARC readiness to real suites instead of vague confidence
claims.

## Overall Posture

PARC is in H0 hardening and is not production-certified. Repository tests show
useful parser, extraction, scan, and failure behavior, but the built-in
preprocessor is incomplete and current `SourcePackage` construction paths do
not all populate target, input, macro, and provenance fields. Linux system
tests are prerequisite-dependent; Apple and Windows have no native CI gate.

## Subsystem Scorecard

- parser entrypoints: fixture-backed
- AST traversal and printing: fixture-backed
- extraction to `SourcePackage`: fixture-backed, with partial/default fields
- scan-first vendored baselines: hermetic regression evidence
- hostile-header recovery: bounded regression evidence
- built-in preprocessor: incomplete, corpus-scoped evidence
- system-header wrappers: host- and prerequisite-dependent evidence
- Apple/Windows: uncertified; no native H0 CI

## Canonical Readiness Anchors

The regression baseline should be checked against these anchors first:

- vendored musl `stdint`
- vendored zlib
- vendored libpng scan
- repo-owned `macro_env_a`
- repo-owned `type_env_b`
- OpenSSL public wrapper extraction
- combined Linux event-loop wrapper extraction

If those anchors stay green and deterministic, they preserve the current test
baseline. They do not implement H1-H5 or prove a production contract.

## What Would Raise Readiness Further

The next meaningful gains would be:

- broader built-in-preprocessor coverage on other hostile width and platform
  gates beyond the libpng family
- more ugly combined system-header clusters
- more repeat-run deterministic scans on large host-dependent surfaces
- clearer unsupported-case diagnostics for the remaining difficult families
