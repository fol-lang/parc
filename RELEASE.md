# Distribution and release policy

This file defines PARC's distribution identity and compatibility rules. It is
included in every Cargo archive.

## Current identity

| Item | Value |
| --- | --- |
| Cargo package | `follang-parc` 0.16.0 |
| Rust library/import | `parc` |
| Edition | Rust 2021 |
| MSRV | Rust 1.89 |
| License | `MIT OR Apache-2.0` |
| Source schema | `follang.parc.source-package`, version 2 |
| ID algorithm | version 1 |
| Certified implementation surface | H2 source frontend |
| H2 implementation baseline | `9585c5977e73795f71d7844bb179f1a2ba613c83` |
| PARC/LINC/GERC dependencies | none; PARC is the first sibling in release order |

The Rust constants `SOURCE_PACKAGE_SCHEMA_ID`,
`SOURCE_PACKAGE_SCHEMA_VERSION`, and `ID_ALGORITHM_VERSION` are the authority
for artifact consumers. The baseline above identifies the source implementation
certified before this distribution-only hardening change. A release tag records
the exact archive-producing commit, including later documentation or packaging
changes.

PARC's certified surface ends at checked source meaning: preprocessing,
parsing, declarations and types, source provenance, diagnostics, completeness,
canonical transport, and source IDs/fingerprints. It does not certify ABI
layout, object or symbol inspection, provider resolution, link planning, Rust
generation, arbitrary host headers, or whole-toolchain behavior.

## Distribution channel

`Cargo.toml` sets `publish = false`. No crates.io name ownership, availability,
or published release is asserted. The supported distribution channel is a
self-contained `.crate` archive produced from an exact Git tag. Consumers use
that archive or the exact tag commit and import the library as `parc`.

`make test-package` builds a candidate archive, unpacks it outside the
repository, runs the archive's promised default library tests and doctests, and
builds/tests a clean external consumer against package `follang-parc` under the
crate name `parc`. It also rejects path dependencies in the packaged manifest.

The Cargo features `repo-tests`, `system-tests`, and `dev-pegviz` only control
repository validation harnesses. The archive deliberately omits their fixture
trees and does not promise that those repository-only test suites can run from
the archive. These switches do not add public library capabilities.

## Compatibility versions

The Cargo package version follows SemVer for the Rust API and documented
behavior. Before 1.0, a breaking Rust API or behavior change requires a minor
version bump; a backwards-compatible fix or additive change may use a patch
bump. After 1.0, normal SemVer major/minor/patch rules apply.

Schema and ID versions are independent compatibility axes:

- Schema v2 is frozen: it is never changed in place. Any incompatible emitted
  shape or semantic change requires a new source schema version, a new frozen
  corpus, and a breaking SemVer bump (minor before 1.0, major after 1.0).
- Changing ID normalization, domain separation, field order, digest algorithm,
  textual prefix, or semantic inputs requires an ID algorithm version bump and
  new golden vectors. Because those IDs are serialized, such a change also
  requires a source-schema bump and the same breaking SemVer bump.
- Compatible implementation fixes that leave canonical bytes, schema meaning,
  and IDs unchanged do not bump schema or ID versions.
- Additive decoder support for an additional, explicitly implemented schema may
  use a compatible SemVer bump only when the existing encoder, decoder, and
  schema-v2 behavior remain unchanged.
- Decoders accept only versions they explicitly implement. A package version
  bump never makes an unknown schema or ID algorithm acceptable.

The MSRV is the `package.rust-version` value in `Cargo.toml` and is exercised by
CI. A patch release does not raise it. Before 1.0, raising the MSRV requires at
least a minor package-version bump; after 1.0 it requires at least a minor bump.
The change must update `Cargo.toml`, this file, the book identity table, and CI
in one commit.

## Release order and clean-upstream rule

Sibling releases or tags are ordered:

1. PARC contract archive/tag.
2. LINC against that exact PARC version and commit.
3. GERC against those exact PARC and LINC versions and commits.
4. FOL after its lock records all three exact revisions.

Never tag a downstream sibling against uncommitted or merely local upstream
state.

Before proposing `follang-parc-v<version>`:

1. merge the candidate to its tracked upstream branch;
2. run `git fetch --tags origin` explicitly and review the fetched state;
3. check out the release branch with a clean worktree;
4. run `make release-check`;
5. review the reported version, tag name, and full commit ID;
6. create the tag/archive manually under the repository's review policy;
7. record that exact tag commit, package version, schema version, and ID
   algorithm version in LINC before any LINC tag.

`make release-check` refuses detached, dirty, untracked, non-upstream, already
tagged, or registry-publishable state. It then runs `make verify`. It performs
no fetch, version edit, commit, tag, push, upload, or publication.
