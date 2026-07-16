//! Exact, canonical JSON transport for the full signed and unsigned C integer domain.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

/// An integer value whose signedness and complete 128-bit magnitude are
/// preserved by the source contract.
///
/// JSON uses an explicit `sign` tag and a canonical decimal string so values
/// above `i128::MAX` are never rounded, truncated, or reinterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "sign", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExactInteger {
    Signed {
        #[serde(with = "signed_decimal")]
        value: i128,
    },
    Unsigned {
        #[serde(with = "unsigned_decimal")]
        value: u128,
    },
}

impl ExactInteger {
    pub const fn signed(value: i128) -> Self {
        Self::Signed { value }
    }

    pub const fn unsigned(value: u128) -> Self {
        Self::Unsigned { value }
    }

    pub const fn as_signed(self) -> Option<i128> {
        match self {
            Self::Signed { value } => Some(value),
            Self::Unsigned { .. } => None,
        }
    }

    pub const fn as_unsigned(self) -> Option<u128> {
        match self {
            Self::Unsigned { value } => Some(value),
            Self::Signed { .. } => None,
        }
    }
}

mod signed_decimal {
    use super::*;

    pub(super) fn serialize<S>(value: &i128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<i128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        validate_decimal(&value, true)
            .then_some(())
            .ok_or_else(|| D::Error::custom("expected canonical decimal i128 string"))?;
        value.parse().map_err(D::Error::custom)
    }
}

mod unsigned_decimal {
    use super::*;

    pub(super) fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        validate_decimal(&value, false)
            .then_some(())
            .ok_or_else(|| D::Error::custom("expected canonical decimal u128 string"))?;
        value.parse().map_err(D::Error::custom)
    }
}

fn validate_decimal(value: &str, signed: bool) -> bool {
    if value.is_empty()
        || value.starts_with('+')
        || (value.starts_with('0') && value.len() != 1)
        || value.starts_with("-0")
    {
        return false;
    }
    value
        .bytes()
        .enumerate()
        .all(|(index, byte)| byte.is_ascii_digit() || (signed && index == 0 && byte == b'-'))
}
