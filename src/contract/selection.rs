use std::collections::BTreeSet;

use thiserror::Error;

use super::DeclarationId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    AllSupported,
    Only(Vec<DeclarationId>),
    OpaqueOnly(Vec<DeclarationId>),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SelectionError {
    #[error("selection must contain at least one declaration")]
    Empty,
    #[error("selection contains duplicate declaration {0}")]
    Duplicate(DeclarationId),
}

impl Selection {
    pub const fn all_supported() -> Self {
        Self::AllSupported
    }

    pub fn only(ids: impl IntoIterator<Item = DeclarationId>) -> Result<Self, SelectionError> {
        Self::checked_ids(ids).map(Self::Only)
    }

    /// Selects record declarations as opaque handles. Their definitions are
    /// not required, but their identity and support status remain checked.
    pub fn opaque(ids: impl IntoIterator<Item = DeclarationId>) -> Result<Self, SelectionError> {
        Self::checked_ids(ids).map(Self::OpaqueOnly)
    }

    fn checked_ids(
        ids: impl IntoIterator<Item = DeclarationId>,
    ) -> Result<Vec<DeclarationId>, SelectionError> {
        let ids: Vec<_> = ids.into_iter().collect();
        if ids.is_empty() {
            return Err(SelectionError::Empty);
        }
        let mut seen = BTreeSet::new();
        for id in &ids {
            if !seen.insert(*id) {
                return Err(SelectionError::Duplicate(*id));
            }
        }
        let mut ids = ids;
        ids.sort_unstable();
        Ok(ids)
    }

    pub fn roots(&self) -> Option<&[DeclarationId]> {
        match self {
            Self::AllSupported => None,
            Self::Only(ids) | Self::OpaqueOnly(ids) => Some(ids),
        }
    }

    pub fn explicit_ids(&self) -> impl Iterator<Item = DeclarationId> + '_ {
        self.roots().into_iter().flatten().copied()
    }
}
