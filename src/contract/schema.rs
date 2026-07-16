use serde::{Deserialize, Serialize};

pub const SOURCE_PACKAGE_KIND: &str = "follang.parc.source-package";
pub const SOURCE_PACKAGE_SCHEMA_ID: &str = "follang.parc.source-package";
pub const SOURCE_PACKAGE_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaHeader {
    pub id: String,
    pub version: u32,
}

impl SchemaHeader {
    pub fn source_package_v2() -> Self {
        Self {
            id: SOURCE_PACKAGE_SCHEMA_ID.to_owned(),
            version: SOURCE_PACKAGE_SCHEMA_VERSION,
        }
    }

    pub fn is_source_package_v2(&self) -> bool {
        self.id == SOURCE_PACKAGE_SCHEMA_ID && self.version == SOURCE_PACKAGE_SCHEMA_VERSION
    }
}
