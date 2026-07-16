use std::{fmt, str::FromStr};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::canonical::CanonicalHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Digest32([u8; 32]);

impl Digest32 {
    pub(crate) const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FingerprintParseError {
    #[error("expected fingerprint prefix '{expected}'")]
    Prefix { expected: &'static str },
    #[error("fingerprint must contain exactly 64 lowercase hexadecimal digits")]
    Shape,
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

fn decode_prefixed(value: &str, prefix: &'static str) -> Result<Digest32, FingerprintParseError> {
    let Some(hex) = value.strip_prefix(prefix) else {
        return Err(FingerprintParseError::Prefix { expected: prefix });
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(FingerprintParseError::Shape);
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_nibble(pair[0]);
        let low = decode_nibble(pair[1]);
        bytes[index] = (high << 4) | low;
    }
    Ok(Digest32::new(bytes))
}

fn decode_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("shape checked before decoding"),
    }
}

macro_rules! fingerprint_type {
    ($name:ident, $prefix:literal, $context:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Digest32);

        impl $name {
            pub(crate) fn derive(bytes: &[u8]) -> Self {
                let mut hasher = CanonicalHasher::new($context);
                hasher.field(bytes);
                Self(Digest32::new(hasher.finish()))
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                self.0.as_bytes()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}{}", $prefix, encode_hex(self.as_bytes()))
            }
        }

        impl FromStr for $name {
            type Err = FingerprintParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                decode_prefixed(value, $prefix).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(D::Error::custom)
            }
        }
    };
}

fingerprint_type!(
    ContentFingerprint,
    "pcontent1_",
    "follang.parc.content-fingerprint.v1"
);
fingerprint_type!(
    TargetFingerprint,
    "ptarget1_",
    "follang.parc.target-fingerprint.v1"
);
fingerprint_type!(
    SourceFingerprint,
    "psource2_",
    "follang.parc.source-package-fingerprint.v2"
);

impl ContentFingerprint {
    pub fn from_content(content: &[u8]) -> Self {
        Self::derive(content)
    }
}
