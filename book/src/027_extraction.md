# Extraction

AST-to-contract extraction is an internal scan stage, not a standalone public
API. This prevents callers from creating a declaration list without the exact
target, source table, effective preprocessing inputs, diagnostics, provenance,
and completeness state required by the contract.

The internal extractor is deterministic and two-pass:

1. index ordinary, tag, and anonymous identities;
2. lower occurrences, declaration kinds, child IDs, types, and attributes.

This permits forward tag and typedef references to use stable semantic IDs.
Incompatible redeclarations and unusable ranges are diagnosed rather than
silently replaced or assigned placeholder spelling.

Use `parc::scan::scan_headers` to produce a checked `SourcePackage`. Use
`parc::parse` or `parc::driver` when only an AST is required.
