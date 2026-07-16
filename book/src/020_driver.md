# Driver API

The `driver` module is the high-level API for file parsing. It runs a system preprocessor, then
parses the resulting text into a `TranslationUnit`.

## Main types

```rust
pub struct Config {
    pub cpp_command: String,
    pub cpp_options: Vec<String>,
    pub flavor: Flavor,
}

pub enum Flavor {
    StdC11,
    GnuC11,
    ClangC11,
}

pub struct Parse {
    pub source: String,
    pub unit: TranslationUnit,
}
```

The return value matters:

- `source` is the preprocessed source PARC actually parsed
- `unit` is the AST root

## Basic file parsing

```rust
use parc::driver::{parse, Config};

let config = Config::default();
let parsed = parse(&config, "examples/demo.c")?;

println!("preprocessed bytes: {}", parsed.source.len());
println!("top-level nodes: {}", parsed.unit.0.len());
# Ok::<(), parc::driver::Error>(())
```

## Configuring the preprocessor

You can override both the preprocessor executable and its arguments.

```rust
use parc::driver::{parse, Config, Flavor};

let config = Config {
    cpp_command: "gcc".into(),
    cpp_options: vec![
        "-E".into(),
        "-Iinclude".into(),
        "-DMODE=2".into(),
        "-nostdinc".into(),
    ],
    flavor: Flavor::GnuC11,
};

let parsed = parse(&config, "src/input.c")?;
# Ok::<(), parc::driver::Error>(())
```

This is the place to inject:

- include directories with `-I...`
- macro definitions with `-D...`
- stricter or more isolated builds with `-nostdinc`

## GCC vs Clang helpers

The convenience constructors also select parser flavor:

```rust
use parc::driver::Config;

let gcc = Config::with_gcc();     // gcc -E, GNU flavor
let clang = Config::with_clang(); // clang -E, Clang flavor
```

Use these when you want the parser flavor to match the syntax accepted by the external
preprocessor.

## Parsing preprocessed text directly

If you already have `.i`-style content, skip `parse` and call `parse_preprocessed`.

```rust
use parc::driver::{parse_preprocessed, Config};

let source = r#"
# 1 "sample.i"
typedef int count_t;
count_t next(count_t x) { return x + 1; }
"#
.to_string();

let parsed = parse_preprocessed(&Config::default(), source)?;
println!("{}", parsed.unit.0.len());
# Ok::<(), parc::driver::SyntaxError>(())
```

## Error model

`driver::parse` returns:

```rust
Result<Parse, parc::driver::Error>
```

The error variants are:

- `PreprocessorError(io::Error)` when the external preprocessor fails
- `SyntaxError(SyntaxError)` when preprocessing succeeded but parsing failed

## Working with syntax errors

`SyntaxError` includes:

- `source`: the preprocessed source
- `line`, `column`, `offset`: the parse failure position in that source
- `expected`: a set of expected tokens

Example:

```rust
use parc::driver::{parse_preprocessed, Config};

let broken = "int main( { return 0; }".to_string();
match parse_preprocessed(&Config::default(), broken) {
    Ok(_) => {}
    Err(err) => {
        eprintln!("parse failed at {}:{}", err.line, err.column);
        eprintln!("expected: {:?}", err.expected);
    }
}
```

If the preprocessed source contains line markers, `SyntaxError::get_location()` can reconstruct the
original file and include stack.

## Built-in preprocessor

PARC includes a scoped built-in preprocessor for controlled syntax workflows.
Use `parse_builtin` instead of `parse` when its maintained surface is sufficient:

```rust
use parc::driver::{parse_builtin, Config};
use std::path::Path;

let config = Config::with_gcc();
let include_paths = vec![Path::new("/usr/include")];
let parsed = parse_builtin(&config, "src/input.c", &include_paths)?;
# Ok::<(), parc::driver::Error>(())
```

The built-in preprocessor implements common:

- Object-like and function-like macros (with `#`, `##`, `__VA_ARGS__`)
- Conditional compilation (`#if`, `#ifdef`, `#ifndef`, `#elif`, `#else`, `#endif`)
- `#include` resolution with configurable search paths
- Include guard detection and optimization
- `defined()` operator in `#if` expressions
- Conditional-expression evaluation (arithmetic, bitwise, logical, ternary)
- Predefined target macros (architecture, OS, GCC compatibility)

## Macro extraction

To extract all `#define` macros from a C file (equivalent to `gcc -dD -E`):

```rust
use parc::driver::capture_macros;
use std::path::Path;

let macros = capture_macros("src/input.c", &[Path::new("/usr/include")])?;
for (name, value) in &macros {
    println!("#define {} {}", name, value);
}
# Ok::<(), parc::driver::Error>(())
```

This driver utility returns active preprocessing macros, including predefined
target macros and macros from included headers. It is not the checked schema-v2
macro inventory. The traced built-in `scan_headers` path records effective
macros with original definition and expansion provenance; external scans do not
claim that evidence and carry `PARC-P0001`.

## Practical advice

- Keep `parsed.source` if you plan to report errors later.
- Use `parse_preprocessed` for deterministic regression tests.
- Prefer explicit `cpp_options` in tools and CI so parse behavior stays reproducible.
- Use `parse_builtin` only for controlled syntax inputs. For a source contract,
  use `scan_headers` with an explicit `TargetSpec` and preprocessor mode.
