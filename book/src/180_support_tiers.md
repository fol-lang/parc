# Support Tiers

This chapter records a practical support posture for PARC’s public surface.

It is meant to help downstream users judge which parts of the crate are the safest long-term
integration points.

## Tier 1: Core Consumer Surface

These are the most important public surfaces to depend on:

- `driver`
- `parse`
- `ast`
- `visit`
- `span`
- `loc`

These modules define the main parsing contract of the crate.

## Tier 2: Debugging And Inspection Surface

These are public and useful, but more inspection-oriented than contract-critical:

- `print`
- `Debug` views of AST nodes
- formatted error text

They are valuable for debugging and tests, but long-lived tooling should still prefer structured
data over formatted strings.

## Tier 3: Contributor-Oriented Knowledge

These are important for contributors but should not be treated as downstream contracts:

- parser file organization under `src/parser/`
- helper-module layout
- incidental internal naming
- current implementation decomposition across grammar files

These details may evolve as the parser changes.

## Consumer guidance

If you are building external tooling on top of PARC, bias toward Tier 1 surfaces first.
Reach for Tier 2 when you need diagnostics or debugging support.
Treat Tier 3 as implementation detail unless you are actively contributing to PARC itself.
