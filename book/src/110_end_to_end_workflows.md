# End-To-End Workflows

This chapter ties the public modules together into practical usage patterns.

## Workflow 0: Produce The Checked Source Contract

```rust,ignore
use parc::scan::{
    scan_headers, PathMapping, PathMappingRule, PreprocessorMode, ScanConfig,
};

let mapping = PathMapping::try_new([
    PathMappingRule::try_new("/absolute/project", "project")?,
])?;
let config = ScanConfig::new(checked_target, mapping, PreprocessorMode::Builtin)?
    .entry_header("/absolute/project/include/demo.h");
let report = scan_headers(&config)?;
let package = report.package();
```

Use this path when another stage needs a `SourcePackage`. Check completeness
before consuming declarations; current generated-source scans are partial.
The remaining workflows are syntax-oriented and do not construct a contract.

## Workflow 1: Parse A Real C File

```rust
use parc::driver::{parse, Config};

let parsed = parse(&Config::default(), "include/demo.h")?;
println!("items: {}", parsed.unit.0.len());
# Ok::<(), parc::driver::Error>(())
```

This is the baseline path when:

- includes matter
- macros matter
- compiler predefined types or macros matter

The result gives you both the AST and the exact preprocessed source PARC saw.

## Workflow 2: Parse A Preprocessed Snapshot

```rust
use parc::driver::{parse_preprocessed, Config};

let source = std::fs::read_to_string("snapshots/demo.i").unwrap();
let parsed = parse_preprocessed(&Config::default(), source)?;
# Ok::<(), parc::driver::SyntaxError>(())
```

Use this when:

- reproducing a parse bug
- building deterministic tests
- integrating with a nonstandard build system

This workflow isolates parser behavior from preprocessor invocation behavior.

## Workflow 3: Parse A Fragment In Tests

```rust
use parc::driver::Flavor;
use parc::parse;

let decl = parse::declaration("typedef unsigned long word_t;", Flavor::StdC11)?;
let expr = parse::expression("ptr->field + 1", Flavor::GnuC11)?;
# Ok::<(), parc::parse::ParseError>(())
```

This is the right workflow for:

- unit tests
- grammar debugging
- editor or language-server experiments

## Workflow 4: Build A Syntax Analyzer

```rust
use parc::driver::{parse, Config};
use parc::visit::{self, Visit};
use parc::{ast, span};

struct ReturnCounter {
    count: usize,
}

impl<'ast> Visit<'ast> for ReturnCounter {
    fn visit_statement(&mut self, node: &'ast ast::Statement, span: &'ast span::Span) {
        if matches!(node, ast::Statement::Return(_)) {
            self.count += 1;
        }
        visit::visit_statement(self, node, span);
    }
}

let parsed = parse(&Config::default(), "src/main.c")?;
let mut counter = ReturnCounter { count: 0 };
counter.visit_translation_unit(&parsed.unit);
println!("return statements: {}", counter.count);
# Ok::<(), parc::driver::Error>(())
```

This is the normal PARC analyzer pattern:

1. parse
2. traverse
3. inspect spans and locations
4. emit your own diagnostics or analysis data

## Workflow 5: Build Diagnostics With Real File Locations

```rust
use parc::driver::{parse, Config};
use parc::loc::get_location_for_offset;

let parsed = parse(&Config::default(), "src/main.c")?;

for item in &parsed.unit.0 {
    let (loc, _) = get_location_for_offset(&parsed.source, item.span.start);
    println!("top-level item starts at {}:{}", loc.file, loc.line);
}
# Ok::<(), parc::driver::Error>(())
```

Use this when your users care about original file locations rather than raw byte offsets in the
preprocessed stream.

## Workflow 6: Debug A New Syntax Form

```rust
use parc::driver::Flavor;
use parc::parse;
use parc::print::Printer;
use parc::visit::Visit;

let expr = parse::expression("({ int x = 1; x + 1; })", Flavor::GnuC11)?;

let mut out = String::new();
Printer::new(&mut out).visit_expression(&expr.node, &expr.span);
println!("{}", out);
# Ok::<(), parc::parse::ParseError>(())
```

This is the most effective loop when exploring unfamiliar AST shapes.

## Workflow 7: Regression-Test A Parse Failure

A practical bug workflow is:

1. capture the smallest failing input
2. decide whether preprocessing is relevant
3. add a `parse_api` test or a reftest
4. patch the grammar
5. verify the printed AST or error outcome

That keeps parser changes concrete and reviewable.
