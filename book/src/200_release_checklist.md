# Future Release Checklist

Registry publication and version changes are deferred to H6. This chapter is a
future release checklist, not a statement that PARC is currently release-ready.

The important release posture is architectural:

- `parc` releases source/frontend behavior
- it does not release binary or Rust-generation policy
- the tested `SourcePackage` contract matters more than parser-internal churn

## Parser changes

Before releasing parser changes:

1. confirm the smallest reproducer has a test
2. confirm the intended flavor coverage is tested
3. confirm the AST shape change is deliberate
4. confirm visitor and printer behavior still make sense

## Book changes

Before releasing documentation changes:

1. confirm the affected public behavior is described in the book
2. confirm unsupported or out-of-scope cases are still documented honestly
3. confirm examples still match the actual public API names

## Error-surface changes

Before releasing changes around errors:

1. confirm structured fields still provide the needed information
2. avoid treating formatted strings as the real contract
3. update the error-surface chapter if the practical behavior changed

## Workflow changes

Before releasing changes to the normal integration path:

1. update the workflow chapter
2. update the API contract chapter if the preferred boundary changed
3. update stable-usage guidance if downstream posture should change

## Artifact contract changes

Before releasing a `SourcePackage` shape change:

1. confirm the changed field meaning is covered by contract-level tests
2. confirm the consuming workflow examples still describe artifact boundaries
3. confirm cross-crate composition is still described as tests/examples/harness
   work, not library coupling

## Current H0 gate

The current non-release baseline uses `make build`, `make fmt-check`,
`make lint`, `make check-features`, `make test`, `make test-contract`,
`make test-package`, `make test-system`, `make docs-check`, and `make verify`.
The system lane is optional for the standalone local command but required by
`make verify` and required CI. Passing these commands establishes H0 feedback;
it does not establish H1-H5 contract or platform certification.

Before any H6 publication decision, re-audit the versioned source artifact,
package metadata, legal files, canonical consumers, and certified target
matrix. Do not infer release readiness from the old corpus list.

## Final practical rule

If a change would force a downstream PARC consumer to rethink how it parses, traverses, or reports
on source, the book should say so explicitly in the same change.
