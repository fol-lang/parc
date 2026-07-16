# Support Tiers

Support claims follow evidence boundaries. Public Rust visibility by itself is
not a claim that a surface belongs to the certified H2 source contract.

## Tier 1: Checked source-contract surface

The preferred downstream interop boundary is:

- `scan::{ScanConfig, scan_headers}` for explicit-target production;
- immutable values under `contract`;
- `contract::{encode_source_package, decode_source_package}` for transport;
- completeness, per-declaration support, diagnostics, selection, `retain`, and
  `merge` checks before consumption.

This tier has the schema-v2 corpus, canonical IDs/fingerprints, bounded H2 scan
evidence, and fail-closed validation described by the hardening matrix. It does
not include ABI layout, symbols, providers, link plans, or Rust generation.

## Tier 2: Syntax-oriented public surface

The parser-oriented modules `driver`, `parse`, `ast`, `visit`, `span`, and `loc`
remain public and have repository fixture coverage. `print` is useful for AST
inspection. These APIs do not create a `SourcePackage`, and successful parsing
does not upgrade an incomplete contract-producing scan to complete.

Consumers that need the checked cross-crate source boundary should stay on Tier
1 instead of rebuilding source meaning from these syntax APIs.

## Tier 3: Repository implementation detail

The following are contributor details, not downstream contracts:

- parser file organization under `src/parser/`;
- the crate-private extractor and canonical wire DTOs;
- repository test features and fixture layout;
- incidental helper-module names and internal decomposition.

Rust API compatibility for public surfaces follows `RELEASE.md`. Schema and ID
compatibility use their own explicit versions; package SemVer never substitutes
for checking artifact versions and completeness.
