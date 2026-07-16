# Project Layout

| Path | Purpose |
| --- | --- |
| `src/contract/` | Immutable checked source contract and canonical codec |
| `src/scan/` | Explicit-target scan configuration and orchestration |
| `src/extract/` | Crate-private deterministic AST-to-contract lowering |
| `src/driver.rs` | File parsing through external preprocessing |
| `src/preprocess/` | Built-in preprocessing and include resolution |
| `src/parse.rs` | Direct and resilient fragment/translation-unit parsing |
| `src/ast/` | Parser AST definitions |
| `src/visit/` | AST traversal |
| `src/parser/` | Grammar implementation |
| `src/span.rs`, `src/loc.rs` | Byte spans and line-marker locations |
| `src/tests/` | API, scan-contract, reftest, and system coverage |

The old `src/ir/` and `src/intake/` trees are gone. Do not add alternate
package-construction routes around `scan` and `contract`.

The parser tracks typedef state in an internal `Env`; resilient recovery resets
that state from the last successfully parsed prefix before continuing. Include
resolution contains no unsafe callback escape hatch; the crate forbids unsafe
code.

Use Make targets for formatting, checks, tests, packaging, system lanes, and
documentation.
