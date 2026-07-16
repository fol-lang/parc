//! Parser-independent, checked source contract for downstream interop stages.
//!
//! Contract values are immutable after construction. Serialized packages must
//! be decoded with [`decode_source_package`]; the domain model deliberately
//! does not implement `Deserialize`.

mod canonical;
mod codec;
mod complete;
pub mod corpus;
mod declarations;
mod diagnostics;
mod exact_integer;
mod files;
mod fingerprint;
mod ids;
mod inputs;
mod macros;
mod package;
mod provenance;
mod report;
mod schema;
mod selection;
mod target;
mod types;
mod validate;
mod wire;

pub use codec::{
    decode_source_package, decode_source_package_with_limits, encode_source_package, DecodeError,
    DecodeLimits, EncodeError,
};
pub use complete::{
    ClosureRequirement, CompleteSourcePackage, CompletionBlocker, DeclarationClosureEntry,
    IncompleteSource,
};
pub use declarations::*;
pub use diagnostics::*;
pub use exact_integer::ExactInteger;
pub use files::*;
pub use fingerprint::{ContentFingerprint, SourceFingerprint, TargetFingerprint};
pub use ids::*;
pub use inputs::*;
pub use macros::*;
pub use package::{SourcePackage, SourcePackageBuildError, SourcePackageInput};
pub use provenance::*;
pub use report::ScanReport;
pub use schema::{
    SchemaHeader, SOURCE_PACKAGE_KIND, SOURCE_PACKAGE_SCHEMA_ID, SOURCE_PACKAGE_SCHEMA_VERSION,
};
pub use selection::{Selection, SelectionError};
pub use target::*;
pub use types::*;
pub use validate::{ContractViolation, ContractViolationCode};

#[cfg(test)]
mod tests;
