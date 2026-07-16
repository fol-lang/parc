use super::{CompleteSourcePackage, IncompleteSource, Selection, SourceDiagnostic, SourcePackage};

/// The checked result of one source scan.
///
/// Diagnostics are owned by the fingerprinted package. Keeping this wrapper
/// immutable prevents a second, divergent diagnostic stream from being
/// presented beside the contract artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    package: SourcePackage,
}

impl ScanReport {
    pub(crate) const fn new(package: SourcePackage) -> Self {
        Self { package }
    }

    pub fn package(&self) -> &SourcePackage {
        &self.package
    }

    pub fn diagnostics(&self) -> &[SourceDiagnostic] {
        self.package.diagnostics()
    }

    pub fn into_package(self) -> SourcePackage {
        self.package
    }

    pub fn into_complete(
        self,
        selected: &Selection,
    ) -> Result<CompleteSourcePackage, IncompleteSource> {
        self.package.into_complete(selected)
    }
}
