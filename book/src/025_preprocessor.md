# Built-in Preprocessor

`parc::preprocess` provides a scoped built-in preprocessing implementation. It
is useful for controlled inputs and repository fixtures; it is not claimed to
be a complete replacement for GCC or Clang on arbitrary system headers.

The contract-producing scan API selects preprocessing explicitly with
`PreprocessorMode::Builtin` or `PreprocessorMode::External`. Built-in scan
macros come from the caller's checked `TargetSpec`; the scan path never derives
a host target.

## Components

| Module | Purpose |
| --- | --- |
| `token`, `lexer` | Preprocessing tokens and tokenization |
| `directive` | Directive parsing |
| `macros` | Object-like and function-like macro state and expansion |
| `expr` | Conditional-expression evaluation |
| `processor` | Conditional compilation and expansion orchestration |
| `include` | Include lookup, caching, and guard tracking |
| `predefined` | Low-level target-macro primitives used by syntax APIs |

## Controlled-source helper

```rust
use parc::preprocess::preprocess;

let output = preprocess("#define X 42\nint a = X;\n");
assert!(output.errors.is_empty());
```

This helper preprocesses text; it does not construct a `SourcePackage`, record
effective target inputs, or prove original macro provenance.

## Implemented surface

The built-in path covers common object-like and function-like macros,
variadics, stringification, token pasting, standard conditional directives,
include lookup, and include guards. Its conditional evaluator handles the
ordinary arithmetic, comparison, logical, bitwise, and conditional operators
used by the maintained fixtures.

Those capabilities are implementation coverage, not a universal compatibility
claim. Compiler-specific predefined macros, unusual token-pasting behavior,
extension-heavy include stacks, and host SDK headers may still require an
external preprocessor or produce an explicit partial/error outcome.

## Include resolution

`IncludeResolver` accepts explicit local and system search paths. The public
H1 scan configuration requires those paths to be absolute, existing, and
covered by `PathMapping`; it records their logical identities in effective
inputs. Include implementation contains no unsafe callback escape hatch.

## Contract limitation

H1 stores exact ranges in the generated preprocessed file, but it does not yet
prove transitive include content or macro-expansion chains. Consequently every
current scan carries `PARC-P0001`, an empty contract macro table, and
`Completeness::Partial`, regardless of which preprocessor mode produced the
generated text.
