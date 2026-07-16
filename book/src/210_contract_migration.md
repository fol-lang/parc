# Contract Migration

Consumers moving from parser-owned extraction or mutable source packages must
adopt the checked PARC schema-v2 boundary:

```text
C entry header -> parc::scan -> parc::contract::SourcePackage -> downstream
```

## Required changes

1. Replace direct AST extraction or mutable package builders with
   `scan::scan_headers`.
2. Construct a checked `contract::TargetSpec` from explicit compiler and C
   data-model evidence.
3. Map absolute source roots to canonical logical roots with `PathMapping`.
4. Consume `SourceDeclarationKind` and `CTypeKind` instead of ad hoc item/type
   projections.
5. Check package completeness and per-declaration support.
6. Move layout, symbol, and link facts to LINC; move Rust lowering to GERC.

The former public `parc::ir`, `parc::intake`, extractor functions,
`SourcePackageBuilder`, in-place filtering, and lossy package merge APIs have no
compatibility layer. Use `SourcePackage::retain` for a checked transitive
declaration projection and `SourcePackage::merge` for target/input-compatible
artifacts; stable-ID conflicts are errors rather than overwrite rules.

## Type references

Types use semantic declaration IDs for aliases, records, and enums. Pointer
qualifiers are attached to their exact pointer layer; pointee qualifiers remain
on the pointee. Function-pointer types retain parameter types, prototype, and
calling convention. Do not flatten these structures to names.

## Artifact handling

Use `contract::decode_source_package` for untrusted bytes and
`contract::encode_source_package` for canonical output. Do not deserialize the
domain model with Serde or copy a stored fingerprint into a new package.
