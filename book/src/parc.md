# PARC Reference

PARC is the source frontend of the toolchain. It has two public surfaces:

1. `scan` and `contract` for checked source artifacts;
2. `driver`, `parse`, `ast`, and `visit` for syntax-level tooling.

## Data flow

```text
one entry header + explicit TargetSpec + explicit source inputs
  -> builtin or checked external preprocessing
  -> resilient parser
  -> internal deterministic lowering
  -> checked contract::SourcePackage
  -> canonical schema-v2 artifact
```

There is no public AST-to-contract shortcut. Contract construction needs the
target, effective preprocessing inputs, source table, diagnostics, and
completeness proof assembled together.

## Ownership

PARC owns preprocessing, parsing, source declarations and types, source
diagnostics, provenance, and artifact identity. LINC owns measured ABI and
binary evidence. GERC owns Rust lowering and generated build output.

The strongest consumer boundary is `parc::contract::SourcePackage`. The former
`ir` and `intake` modules are not compatibility paths.

## Reading strategy

- Contract consumers: Source Contract, Header Scanning, API Contract.
- Parser consumers: Driver API, Parser API, AST Model, Visitor Pattern.
- Contributors: Project Layout, Testing, Parser Boundaries.
