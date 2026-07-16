//! Embedded, package-safe access to the shared H1 preservation corpus.

/// One PARC-owned source-package artifact in the cross-repository corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreservationCase {
    pub name: &'static str,
    pub envelope_json: &'static [u8],
}

pub const COMPLETE_SOURCE_PACKAGE_JSON: &[u8] =
    include_bytes!("../../contract-corpus/v2/preservation/source-complete.json");
pub const PARTIAL_SOURCE_PACKAGE_JSON: &[u8] =
    include_bytes!("../../contract-corpus/v2/preservation/source-partial.json");
pub const PRESERVATION_LEDGER_JSON: &str =
    include_str!("../../contract-corpus/v2/preservation/ledger.json");
pub const ID_GOLDEN_VECTORS_JSON: &str =
    include_str!("../../contract-corpus/v2/preservation/id-golden-vectors.json");
pub const PRESERVATION_HEADER: &str =
    include_str!("../../contract-corpus/v2/preservation/input/preservation.h");

static CASES: [PreservationCase; 2] = [
    PreservationCase {
        name: "complete",
        envelope_json: COMPLETE_SOURCE_PACKAGE_JSON,
    },
    PreservationCase {
        name: "partial",
        envelope_json: PARTIAL_SOURCE_PACKAGE_JSON,
    },
];

pub const fn preservation_cases() -> &'static [PreservationCase] {
    &CASES
}
