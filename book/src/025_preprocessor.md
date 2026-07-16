# Built-in Preprocessor

`parc::preprocess` provides a scoped built-in preprocessing implementation. It
is useful for controlled inputs and repository fixtures; it is not claimed to
be a complete replacement for GCC or Clang on arbitrary system headers.

The contract-producing scan API selects preprocessing explicitly with
`PreprocessorMode::Builtin` or `PreprocessorMode::External`. Built-in scan
macros come from the caller's checked `TargetSpec`; the scan path never derives
a host target. `ScanLimits` bounds file and total input, include depth/count,
macro definitions/expansions, nested macro-expansion depth, tokens, generated
bytes, external output, and external runtime.

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

The low-level text helper covers common object-like and function-like macros,
variadics, stringification, token pasting, standard conditional directives,
include lookup, and include guards. The contract-producing tracer deliberately
has a narrower certified surface: object-like and function-like expansion,
checked signed conditional arithmetic, explicit include lookup, header guards,
and `#pragma once`. It rejects unproved `#`/`##`, unsupported directives and
pragmas, malformed macro arity, representation-dependent conditional
expressions, and unsafe include paths with exact diagnostics.

Those capabilities are implementation coverage, not a universal compatibility
claim. Compiler-specific predefined macros, unusual token-pasting behavior,
extension-heavy include stacks, and host SDK headers may still require an
external preprocessor or produce an explicit partial/error outcome.

## Include resolution

`IncludeResolver` accepts explicit local and system search paths. The public
scan configuration requires those paths to be absolute, existing, and
covered by `PathMapping`; it records their logical identities in effective
inputs. Canonicalized roots and resolved symlink targets may not escape the
mapping policy. Include implementation contains no unsafe callback escape
hatch.

## Contract evidence boundary

The traced built-in path attaches declarations and macros to original logical
files and records transitive content identities, include chains, and macro
definition/invocation ranges. A scan is `Complete` only inside that exact,
bounded surface. The external path records its compiler identity, argv,
sysroot, environment policy, and generated source, but remains `Partial` with
`PARC-P0001`; external output does not prove original file or macro provenance.
