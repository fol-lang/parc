mod closure_ledger;
mod consumability;
mod contract;
#[cfg(feature = "repo-tests")]
mod determinism;
mod differential;
#[cfg(any(feature = "repo-tests", feature = "system-tests"))]
mod external_tools;
mod extraction_fixtures;
#[cfg(feature = "repo-tests")]
mod failure_matrix_preprocess;
mod failure_matrix_source;
#[cfg(any(feature = "repo-tests", feature = "system-tests"))]
mod full_apps;
#[cfg(any(feature = "repo-tests", feature = "system-tests"))]
mod hostile_corpus;
mod hostile_headers;
mod parse_api;
mod recovery;
#[cfg(feature = "repo-tests")]
mod reftests;
#[cfg(feature = "repo-tests")]
mod scan_multifile;
#[cfg(any(feature = "repo-tests", feature = "system-tests"))]
mod support;
#[cfg(feature = "system-tests")]
mod system_headers;
#[cfg(feature = "system-tests")]
pub(crate) mod system_support;
