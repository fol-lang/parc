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

H1 accepts exactly one entry header. Scan separate translation units
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

## Result and current completeness

`scan_headers` returns a `ScanReport`; diagnostics live only in its immutable
package. Current declarations use exact ranges in a generated preprocessed
`SourceFile` with `SourceOrigin::Generated`. Because original include and macro
provenance is not yet proven, `PARC-P0001` forces every H1 scan to
`Completeness::Partial`. Macros and transitive include content are not claimed
as complete under that state.

Operational setup errors return `ScanError`. Parser recovery is structured and
becomes a forcing diagnostic with the skipped generated-source range.
