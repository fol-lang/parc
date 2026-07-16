# Reproducibility

Parsing C is sensitive to the exact preprocessor environment.

This chapter documents how to keep PARC-based workflows reproducible.

## Main reproducibility risks

The biggest sources of drift are:

- different preprocessor executables
- different default include paths
- different predefined macros
- different parser flavor settings
- different preprocessed snapshots in tests

## Best practices

For durable automation:

1. construct scan `TargetSpec` from checked compiler and data-model evidence
2. map every absolute operational path to a canonical logical path
3. use `EnvironmentPolicy::Hermetic` or capture named variables explicitly
4. fingerprint-check an external preprocessor against `TargetSpec`
5. keep preprocessed snapshots for syntax-only parser regressions

## Deterministic parse debugging

If a real file parse is inconsistent across machines, a strong debugging move is:

1. capture the preprocessed output
2. switch the failing test to `parse_preprocessed`
3. debug PARC against the stable snapshot

That separates:

- preprocessing differences
- parser differences

## Reftests and snapshots

The reftest harness already encourages deterministic expectations by comparing against printed AST
output. For parser bugs that depend on preprocessing, a pinned `.i` file is often even better.

## Consumer guidance

If PARC is part of a larger pipeline, keep the following recorded somewhere durable:

- target and compiler fingerprints
- logical path mapping and preprocessor arguments
- environment policy and captured-variable fingerprints
- representative fixtures
- expected package fingerprint, completeness, and diagnostics

Without that context, debugging parser regressions is much slower.
