# Header Scanning

`parc::scan` is the public source-contract producer.

## Required configuration

`ScanConfig::new` requires a checked `TargetSpec`, a `PathMapping`, and a
`PreprocessorMode`. There is no host/default target or implicit environment.

```rust,ignore
use parc::scan::{scan_headers, PathMapping, PathMappingRule, PreprocessorMode, ScanConfig};

let mapping = PathMapping::try_new([
    PathMappingRule::try_new("/absolute/project", "project")?,
])?;
let config = ScanConfig::new(target, mapping, PreprocessorMode::Builtin)?
    .entry_header("/absolute/project/include/api.h");
let report = scan_headers(&config)?;
let package = report.package();
```

All source paths are absolute operational paths covered by canonical mapping
rules. Physical roots are excluded from identity; logical paths and the mapping
fingerprint are included. Duplicate or overlapping roots are rejected.

Each scan accepts exactly one entry header. Scan separate translation units
independently so their typedef and file-scope namespaces cannot leak or collide.

## Environment policy

`EnvironmentPolicy::Hermetic` is the default. `EnvironmentPolicy::captured`
accepts an explicit sorted set of variable names. Requested-but-unset variables
are recorded distinctly, non-UTF values are rejected, and the external process
starts from `env_clear()`.

## Preprocessors

- `Builtin` uses explicit target macros and configured include paths.
- `External { executable }` requires an absolute file whose content fingerprint
  equals the compiler executable fingerprint in `TargetSpec`. Recorded argv
  paths are logical, while physical paths remain operational only.

Both modes honor C11 and C17 target standards. Define and undefine events are
validated before they can become argv.

`ScanLimits` may tighten, but never raise, the production ceilings for input
bytes, include recursion/count, macro work and nesting depth, tokens, generated
output, external output, and external runtime. Hitting a ceiling is a named
forcing diagnostic or a bounded `ScanError` when no truthful truncated artifact
can be built.

## Result and completeness

`scan_headers` returns a `ScanReport`; diagnostics live only in its immutable
package. The traced built-in path remaps declaration, child, attribute,
diagnostic, and recovery ranges to original logical files. It also populates
transitive files, effective macros, include provenance, and macro expansions.
Any unmappable range fails closed with `PARC-P2000`; it cannot silently become
complete. A fully modeled built-in translation unit may therefore be
`Completeness::Complete`.

External declarations remain attached to a generated file and `PARC-P0001`
forces `Partial`, because original provenance cannot be recovered from compiler
text alone. Operational setup errors return `ScanError`. Parser recovery is
structured and always has a forcing completeness effect.
