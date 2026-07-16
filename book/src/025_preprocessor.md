# Built-in Preprocessor

PARC includes a built-in C preprocessor in the `parc::preprocess` module. It is
useful for controlled inputs and repository fixtures, but it is incomplete and
does not claim parity with GCC, Clang, or arbitrary system-header stacks. The
normal external-driver path still requires a working `gcc` or `clang` when that
mode is selected.

## Architecture

The preprocessor is split into focused modules:

| Module | Purpose |
|--------|---------|
| `token` | Token types (`Ident`, `Number`, `Punct`, etc.) |
| `lexer` | Preprocessor tokenizer (§6.4 preprocessing tokens) |
| `directive` | Directive parser (`#define`, `#if`, `#include`, etc.) |
| `macros` | Macro table, object-like and function-like expansion |
| `expr` | `#if` constant expression evaluator |
| `processor` | Conditional compilation engine |
| `include` | `#include` resolution with search paths and guard tracking |
| `predefined` | Target-specific predefined macros |

## Quick start

```rust
use parc::preprocess::preprocess;

let output = preprocess("#define X 42\nint a = X;\n");
// output.tokens contains the expanded token stream
```

## Macro expansion

Both object-like and function-like macros are supported:

```c
#define SIZE 1024
#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define LOG(fmt, ...) printf(fmt, __VA_ARGS__)
```

Features:
- `#` stringification operator
- `##` token pasting operator
- `__VA_ARGS__` for variadic macros
- Recursive expansion with "paint set" to prevent infinite recursion (C standard §6.10.3.4)
- Self-referential macros handled correctly (`#define X X + 1` expands to `X + 1`)

## Conditional compilation

The current conditional directive parser recognizes:

```c
#if CONDITION
#ifdef NAME
#ifndef NAME
#elif CONDITION
#else
#endif
```

The fixture-covered `#if` expression evaluator includes:
- Integer literals (decimal, octal, hex, binary)
- Character constants (`'x'`)
- `defined(NAME)` and `defined NAME`
- All C operators: arithmetic, bitwise, logical, comparison, ternary
- Undefined identifiers evaluate to `0` (per C standard §6.10.1p4)

## Include resolution

```rust
use parc::preprocess::{IncludeResolver, Processor};

let mut resolver = IncludeResolver::new();
resolver.add_system_path("/usr/include");
resolver.add_local_path("./include");

let mut processor = Processor::new();
let result = resolver.preprocess_file(
    std::path::Path::new("src/main.c"),
    &mut processor,
);
```

Features:
- `"local"` includes search relative to the including file, then local paths
- `<system>` includes search system paths only
- Include guard detection (`#ifndef X / #define X / ... / #endif`)
- File content caching
- Maximum include depth (200) to prevent infinite recursion

## Predefined macros

Target-specific macros are available for common platforms:

```rust
use parc::preprocess::{MacroTable, Target, define_target_macros};

let mut table = MacroTable::new();
define_target_macros(&mut table, &Target::host());
// Now table has __STDC__, __linux__, __x86_64__, __GNUC__, etc.
```

Predefined-macro configurations currently exist for:
- **Architectures**: x86_64, aarch64, x86, arm
- **Operating-system macro sets**: Linux, macOS (Darwin), Windows

These configurations only select predefined macro values. They do not certify
native parsing or preprocessing support for Apple or Windows, and H0 has no
native CI gate for either platform.

Standard macros defined:
- `__STDC__`, `__STDC_VERSION__`, `__STDC_HOSTED__`
- Architecture-specific: `__x86_64__`, `__aarch64__`, `__i386__`, `__arm__`
- OS-specific: `__linux__`, `__APPLE__`, `_WIN32`, etc.
- GCC compatibility: `__GNUC__`, `__GNUC_MINOR__`, `__GNUC_PATCHLEVEL__`
- Type sizes: `__SIZEOF_POINTER__`, `__SIZEOF_INT__`, etc.
- Limits: `__CHAR_BIT__`, `__INT_MAX__`, `__LONG_MAX__`
