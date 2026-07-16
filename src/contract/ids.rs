use std::{fmt, str::FromStr};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

use super::{
    canonical::{push_field, CanonicalHasher},
    fingerprint::{Digest32, FingerprintParseError},
};

pub(crate) fn canonical_tokens_bytes(tokens: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for token in tokens {
        push_field(&mut bytes, token.as_bytes());
    }
    bytes
}

pub const ID_ALGORITHM_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("logical path must be nonempty and relative")]
    InvalidLogicalPath,
    #[error("identifier name must be nonempty")]
    EmptyName,
    #[error("parameter child identity requires a semantic ordinal")]
    ParameterRequiresOrdinal,
    #[error(transparent)]
    InvalidEncoding(#[from] FingerprintParseError),
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

fn parse_digest(value: &str, prefix: &'static str) -> Result<Digest32, IdError> {
    let Some(hex) = value.strip_prefix(prefix) else {
        return Err(FingerprintParseError::Prefix { expected: prefix }.into());
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(FingerprintParseError::Shape.into());
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let nibble = |byte| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => unreachable!("shape checked"),
        };
        bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Ok(Digest32::new(bytes))
}

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Digest32);

        impl $name {
            pub const fn as_bytes(&self) -> &[u8; 32] {
                self.0.as_bytes()
            }

            pub(crate) const fn from_digest(digest: Digest32) -> Self {
                Self(digest)
            }

            #[allow(dead_code)]
            pub(crate) const fn digest(self) -> Digest32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}{}", $prefix, encode_hex(self.as_bytes()))
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse_digest(value, $prefix).map(Self)
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

id_type!(FileId, "pfile1_");
id_type!(EntityId, "pentity1_");
id_type!(DeclarationId, "pdecl1_");
id_type!(ChildId, "pchild1_");
id_type!(MacroId, "pmacro1_");
id_type!(OccurrenceId, "pocc1_");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityNamespace {
    Ordinary,
    Tag,
}

impl EntityNamespace {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Ordinary => b"ordinary",
            Self::Tag => b"tag",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EntityScope {
    TranslationUnit,
    File(FileId),
    Owner(DeclarationId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildRole {
    Parameter,
    Field,
    EnumVariant,
}

impl ChildRole {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Parameter => b"parameter",
            Self::Field => b"field",
            Self::EnumVariant => b"enum-variant",
        }
    }
}

pub fn normalize_identifier(name: &str) -> Result<String, IdError> {
    let normalized: String = name.nfc().collect();
    if normalized.is_empty() {
        Err(IdError::EmptyName)
    } else {
        Ok(normalized)
    }
}

pub fn normalize_logical_path(path: &str) -> Result<String, IdError> {
    let path = path.replace('\\', "/");
    if path.is_empty() || path.starts_with('/') || path.contains('\0') {
        return Err(IdError::InvalidLogicalPath);
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(IdError::InvalidLogicalPath),
            other => components.push(other),
        }
    }
    if components.is_empty() || components[0].contains(':') {
        return Err(IdError::InvalidLogicalPath);
    }
    Ok(components.join("/").nfc().collect())
}

impl FileId {
    pub fn from_logical_path(path: &str) -> Result<Self, IdError> {
        let path = normalize_logical_path(path)?;
        let mut hasher = CanonicalHasher::new("follang.parc.file-id.v1");
        hasher.field(path.as_bytes());
        Ok(Self::from_digest(Digest32::new(hasher.finish())))
    }
}

impl EntityId {
    pub fn named(
        namespace: EntityNamespace,
        scope: EntityScope,
        name: &str,
    ) -> Result<Self, IdError> {
        let name = normalize_identifier(name)?;
        let mut hasher = CanonicalHasher::new("follang.parc.entity-id.v1");
        hasher.field(namespace.label());
        hash_scope(&mut hasher, scope);
        hasher.field(name.as_bytes());
        Ok(Self::from_digest(Digest32::new(hasher.finish())))
    }

    pub fn anonymous(scope: EntityScope, token_digest: &[u8], ordinal: u64) -> Self {
        let mut hasher = CanonicalHasher::new("follang.parc.entity-id.v1");
        hasher.field(b"anonymous");
        hash_scope(&mut hasher, scope);
        hasher.field(token_digest);
        hasher.field(ordinal.to_le_bytes());
        Self::from_digest(Digest32::new(hasher.finish()))
    }
}

fn hash_scope(hasher: &mut CanonicalHasher, scope: EntityScope) {
    match scope {
        EntityScope::TranslationUnit => hasher.field(b"translation-unit"),
        EntityScope::File(id) => {
            hasher.field(b"file");
            hasher.field(id.as_bytes());
        }
        EntityScope::Owner(id) => {
            hasher.field(b"owner");
            hasher.field(id.as_bytes());
        }
    }
}

impl DeclarationId {
    pub const fn from_entity(entity: EntityId) -> Self {
        Self::from_digest(entity.digest())
    }

    pub const fn entity(self) -> EntityId {
        EntityId::from_digest(self.digest())
    }
}

impl MacroId {
    pub fn named(file: FileId, name: &str) -> Result<Self, IdError> {
        let name = normalize_identifier(name)?;
        let mut hasher = CanonicalHasher::new("follang.parc.macro-id.v1");
        hasher.field(file.as_bytes());
        hasher.field(name.as_bytes());
        Ok(Self::from_digest(Digest32::new(hasher.finish())))
    }
}

impl OccurrenceId {
    pub fn derive(
        declaration: DeclarationId,
        file: FileId,
        normalized_tokens: &[u8],
        duplicate_ordinal: u64,
    ) -> Self {
        let mut hasher = CanonicalHasher::new("follang.parc.occurrence-id.v1");
        hasher.field(declaration.as_bytes());
        hasher.field(file.as_bytes());
        hasher.field(normalized_tokens);
        hasher.field(duplicate_ordinal.to_le_bytes());
        Self::from_digest(Digest32::new(hasher.finish()))
    }

    pub fn derive_macro(
        macro_id: MacroId,
        file: FileId,
        normalized_tokens: &[u8],
        duplicate_ordinal: u64,
    ) -> Self {
        let mut hasher = CanonicalHasher::new("follang.parc.occurrence-id.v1");
        hasher.field(macro_id.as_bytes());
        hasher.field(file.as_bytes());
        hasher.field(normalized_tokens);
        hasher.field(duplicate_ordinal.to_le_bytes());
        Self::from_digest(Digest32::new(hasher.finish()))
    }
}

impl ChildId {
    /// Derives a parameter identity from its ABI-semantic prototype position,
    /// independent of an optional, non-semantic parameter spelling.
    pub fn parameter(parent: DeclarationId, ordinal: u64) -> Self {
        Self::derive(parent, ChildRole::Parameter, b"semantic-ordinal", ordinal)
    }

    pub fn named(parent: DeclarationId, role: ChildRole, name: &str) -> Result<Self, IdError> {
        if role == ChildRole::Parameter {
            return Err(IdError::ParameterRequiresOrdinal);
        }
        let name = normalize_identifier(name)?;
        Ok(Self::derive(parent, role, name.as_bytes(), 0))
    }

    pub fn anonymous(
        parent: DeclarationId,
        role: ChildRole,
        normalized_tokens: &[u8],
        duplicate_ordinal: u64,
    ) -> Self {
        Self::derive(parent, role, normalized_tokens, duplicate_ordinal)
    }

    fn derive(parent: DeclarationId, role: ChildRole, key: &[u8], ordinal: u64) -> Self {
        let mut hasher = CanonicalHasher::new("follang.parc.child-id.v1");
        hasher.field(parent.as_bytes());
        hasher.field(role.label());
        hasher.field(key);
        hasher.field(ordinal.to_le_bytes());
        Self::from_digest(Digest32::new(hasher.finish()))
    }
}
