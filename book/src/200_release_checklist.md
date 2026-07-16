# Release Policy and Checklist

The root `RELEASE.md`, which is included in the package archive, is the
normative distribution policy. This chapter summarizes the repository checks.

## Identity and compatibility

The current identities are:

- package `follang-parc` 0.16.0, imported as `parc`;
- MSRV Rust 1.89;
- source schema `follang.parc.source-package` version 2;
- ID algorithm version 1;
- H2 implementation baseline
  `9585c5977e73795f71d7844bb179f1a2ba613c83`;
- no PARC/LINC/GERC dependency, because PARC is the first sibling released.

Registry publication is disabled. The project does not claim crates.io name
ownership or availability. Distribution uses an exact Git tag and its tested
self-contained Cargo archive.

Rust API/behavior changes follow SemVer. Before 1.0, breaking changes require a
minor bump. Frozen schema v2 is never changed in place: incompatible artifact
changes require a new schema, corpus, and breaking SemVer bump. ID
normalization/domain/input changes require a new ID algorithm, golden vectors,
schema, and breaking SemVer bump because IDs are serialized. A patch release
does not raise the MSRV; detailed pre/post-1.0 rules are in `RELEASE.md`.

## Change review

Before a release candidate:

1. put parser changes behind the smallest relevant API/reftest fixture;
2. cover contract meaning, canonical bytes, IDs, validation, or completeness in
   contract tests and the frozen corpus as applicable;
3. keep visitor/printer and book descriptions aligned with intentional AST/API
   changes;
4. retain structured fail-closed diagnostics for unsupported source behavior;
5. confirm the book still limits certification to the H2 source frontend.

## Candidate gate

The operator must first fetch and review the tracked upstream and tags. On a
clean branch whose `HEAD` exactly equals its tracked upstream, run:

```sh
make release-check
```

The target refuses detached, dirty, untracked, non-upstream, already-tagged, or
registry-publishable state, then runs the full `make verify` gate. It is
non-mutating: it does not fetch, edit a version, commit, tag, push, upload, or
publish.

The full gate proves:

- formatting, Clippy, feature combinations, repository tests, and doctests;
- frozen schema-v2 corpus and scan preservation;
- required compiler-dependent evidence with no zero-test filter;
- candidate archive default tests/doctests and a clean packaged consumer;
- mdBook and Rust API documentation;
- no worktree change during verification.

## Dependency order

Record the full PARC tag commit, package version, schema version, and ID
algorithm version before downstream tagging. Then release/tag LINC against that
exact PARC state, GERC against both exact upstream states, and finally update
FOL's lock. Never tag a downstream crate against uncommitted or local-only
upstream state.
