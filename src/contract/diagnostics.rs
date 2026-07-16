use std::{fmt, str::FromStr};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::{DeclarationId, SourceRange, TargetFingerprint};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("diagnostic code must match PARC-[EWP][0-9][0-9][0-9][0-9]")]
pub struct InvalidDiagnosticCode;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticCode(String);

impl DiagnosticCode {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidDiagnosticCode> {
        let value = value.into();
        let bytes = value.as_bytes();
        if bytes.len() == 10
            && &bytes[..5] == b"PARC-"
            && matches!(bytes[5], b'E' | b'W' | b'P')
            && bytes[6..].iter().all(u8::is_ascii_digit)
        {
            Ok(Self(value))
        } else {
            Err(InvalidDiagnosticCode)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for DiagnosticCode {
    type Err = InvalidDiagnosticCode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl Serialize for DiagnosticCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DiagnosticCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStage {
    Configuration,
    Preprocess,
    Parse,
    Recovery,
    Extract,
    Contract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Note,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelatedSource {
    pub message: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDiagnostic {
    pub code: DiagnosticCode,
    pub stage: DiagnosticStage,
    pub severity: Severity,
    /// Explicit effect on the package completeness proof. Scanners must use a
    /// forcing value for missing includes, unsupported directives, budget
    /// truncation, and every recovery event.
    pub completeness_impact: DiagnosticCompletenessImpact,
    pub message: String,
    pub range: Option<SourceRange>,
    pub related: Vec<RelatedSource>,
    pub declaration: Option<DeclarationId>,
    pub target: TargetFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCompletenessImpact {
    Informational,
    ForcesPartial,
    ForcesRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SupportStatus {
    Supported,
    Partial {
        code: DiagnosticCode,
        reason: String,
    },
    Unsupported {
        code: DiagnosticCode,
        reason: String,
    },
}

impl SupportStatus {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Completeness {
    Complete,
    Partial { reasons: Vec<CompletenessReason> },
    Rejected { reasons: Vec<CompletenessReason> },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletenessReason {
    pub code: DiagnosticCode,
    pub message: String,
    pub range: Option<SourceRange>,
}
