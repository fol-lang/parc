# Source Contract

`parc::contract` is the parser-independent schema-v2 boundary. Its domain model
is immutable and cannot be deserialized or patched directly.

## Main values

- `TargetSpec` records checked compiler, target, language, extension, C
  data-model, sysroot, and ordered ABI-argument facts.
- `SourcePackage` contains files, effective source inputs, declarations,
  macros, diagnostics, completeness, and a derived source fingerprint.
- `SourceDeclaration` represents functions, records, enums, aliases,
  variables, and explicitly unsupported declarations.
- `CType` preserves source type structure, qualifiers, nullability, references,
  arrays, function types, and support state.
- `SourceRange` always refers to a file in the package table.

Measured record layout, field offsets, enum representation, symbols, and link
facts are deliberately absent.

## Checked construction and decoding

`SourcePackage::try_new` validates all cross-references and canonical ordering
before deriving the package fingerprint. Normal consumers receive a package
from `scan_headers` or decode a canonical envelope:

```rust,ignore
use parc::contract::{decode_source_package, encode_source_package};

let package = decode_source_package(bytes)?;
let canonical = encode_source_package(&package)?;
```

The domain type does not implement `Deserialize`. The decoder validates schema
identity, target identity, IDs, ranges, provenance, completeness, and the
stored fingerprint.

## Completeness

`Completeness` is `Complete`, `Partial`, or `Rejected`. A partial or rejected
package carries exact reasons derived from forcing diagnostics. Use
`SourcePackage::into_complete` (or `ScanReport::into_complete`) with a
`Selection`; never infer completeness from an empty error list.
