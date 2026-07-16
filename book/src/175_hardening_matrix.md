# Hardening Matrix

This chapter translates the large PARC test surface into an explicit hardening
ladder.

The important point is not "how many tests exist". The important point is which
surfaces are carrying confidence for real-header parsing, preprocessing, and
source extraction.

## How To Read The Matrix

Read each surface on three axes:

- hermetic or host-dependent
- parser-only versus scan-first
- success path versus conservative failure path

A surface is stronger when it is:

- hermetic
- scan-first
- repeated deterministically
- tied to a realistic system or library family

## Hermetic Regression Baselines

These are the first surfaces that should stay green on every machine:

- vendored musl `stdint`
- vendored zlib
- vendored libpng builtin-preprocessor success path
- repo-owned `macro_env_a` hostile macro corpus
- repo-owned `type_env_b` hostile type corpus
- parser and extraction corpus fixtures under `src/tests/**`

These matter because they exercise:

- multi-header scanning
- macro and include handling
- extraction into `SourcePackage`
- deterministic behavior without relying on the host toolchain layout

## Host-Dependent Regression Ladders

These should stay green on developer and CI hosts where the headers exist, but
they are not the first portability baseline:

- OpenSSL public wrapper extraction
- combined Linux event-loop wrapper extraction
- larger libc and system-header clusters

These surfaces matter because they are closer to the "real ugly header world"
target than the small synthetic fixtures.

## Tier 3: Hostile And Conservative-Failure Surfaces

These prove that PARC is refusing or degrading honestly instead of pretending to
understand everything:

- hostile declaration fixtures
- repo-owned hostile corpora that force builtin-preprocessor macro and typedef expansion
- recovery fixtures
- unsupported or partial declaration families that still emit diagnostics and
  partial metadata
- extraction-status summaries that distinguish supported, partial, and
  unsupported output trust

For hardening purposes, these failures are useful when they are:

- deterministic
- diagnostic
- documented

## Determinism Anchors

The most important repeat-run anchors right now are:

- vendored musl scan
- vendored zlib scan
- vendored libpng scan
- `macro_env_a` scan
- `type_env_b` scan
- OpenSSL wrapper extraction
- combined Linux event-loop wrapper extraction

If any of those become unstable, confidence in the current baseline should
drop immediately.

## What This Matrix Does Not Mean

This matrix does not mean:

- every random system library now parses perfectly
- every preprocessor corner is solved
- every large host-dependent surface is equally mature

It means the current test-evidence ladder is explicit instead of implied. It
does not complete H1 through H5 or certify any platform.
