//! Top-level source package for the PARC frontend contract.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::diagnostics::SourceDiagnostic;
use super::items::{
    SourceEnum, SourceFunction, SourceItem, SourceItemKind, SourceRecord, SourceTypeAlias,
    SourceUnsupported, SourceVariable,
};
use super::macros::SourceMacro;
use super::provenance::{DeclarationProvenance, MacroProvenance};
use super::target::{SourceInputs, SourceTarget};
use super::types::SourceType;

pub const SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// High-level degradation and trust summary for a source package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionStatus {
    pub item_count: usize,
    pub trustworthy_item_count: usize,
    pub partial_diagnostic_count: usize,
    pub unsupported_diagnostic_count: usize,
    pub parse_failure_count: usize,
}

/// The top-level frontend contract produced by PARC.
///
/// Contains all source-level information extracted from C headers:
/// declarations, macros, diagnostics, and provenance. Independent of
/// any link/binary/ABI-proof concerns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourcePackage {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub target: SourceTarget,
    #[serde(default)]
    pub inputs: SourceInputs,
    #[serde(default)]
    pub items: Vec<SourceItem>,
    #[serde(default)]
    pub macros: Vec<SourceMacro>,
    #[serde(default)]
    pub diagnostics: Vec<SourceDiagnostic>,
    #[serde(default)]
    pub provenance: Vec<DeclarationProvenance>,
    #[serde(default)]
    pub macro_provenance: Vec<MacroProvenance>,
    #[serde(default)]
    pub source_path: Option<String>,
}

impl SourcePackage {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            target: SourceTarget::default(),
            inputs: SourceInputs::default(),
            items: Vec::new(),
            macros: Vec::new(),
            diagnostics: Vec::new(),
            provenance: Vec::new(),
            macro_provenance: Vec::new(),
            source_path: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.diagnostics.is_empty() && self.macros.is_empty()
    }

    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn functions(&self) -> impl Iterator<Item = &SourceFunction> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::Function(f) => Some(f),
            _ => None,
        })
    }

    pub fn records(&self) -> impl Iterator<Item = &SourceRecord> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::Record(r) => Some(r),
            _ => None,
        })
    }

    pub fn enums(&self) -> impl Iterator<Item = &SourceEnum> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::Enum(e) => Some(e),
            _ => None,
        })
    }

    pub fn type_aliases(&self) -> impl Iterator<Item = &SourceTypeAlias> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::TypeAlias(t) => Some(t),
            _ => None,
        })
    }

    pub fn variables(&self) -> impl Iterator<Item = &SourceVariable> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::Variable(v) => Some(v),
            _ => None,
        })
    }

    pub fn unsupported_items(&self) -> impl Iterator<Item = &SourceUnsupported> {
        self.items.iter().filter_map(|item| match item {
            SourceItem::Unsupported(u) => Some(u),
            _ => None,
        })
    }

    pub fn find_function(&self, name: &str) -> Option<&SourceFunction> {
        self.functions().find(|f| f.name == name)
    }

    pub fn find_record(&self, name: &str) -> Option<&SourceRecord> {
        self.records().find(|r| r.name.as_deref() == Some(name))
    }

    pub fn find_enum(&self, name: &str) -> Option<&SourceEnum> {
        self.enums().find(|e| e.name.as_deref() == Some(name))
    }

    pub fn find_type_alias(&self, name: &str) -> Option<&SourceTypeAlias> {
        self.type_aliases().find(|t| t.name == name)
    }

    pub fn find_variable(&self, name: &str) -> Option<&SourceVariable> {
        self.variables().find(|v| v.name == name)
    }

    pub fn find_unsupported(&self, name: &str) -> Option<&SourceUnsupported> {
        self.unsupported_items()
            .find(|u| u.name.as_deref() == Some(name))
    }

    pub fn find_macro(&self, name: &str) -> Option<&SourceMacro> {
        self.macros.iter().find(|m| m.name == name)
    }

    pub fn count_by_kind(&self) -> std::collections::HashMap<SourceItemKind, usize> {
        let mut counts = std::collections::HashMap::new();
        for item in &self.items {
            *counts.entry(item.kind()).or_insert(0) += 1;
        }
        counts
    }

    pub fn function_count(&self) -> usize {
        self.functions().count()
    }

    pub fn record_count(&self) -> usize {
        self.records().count()
    }

    pub fn enum_count(&self) -> usize {
        self.enums().count()
    }

    pub fn type_alias_count(&self) -> usize {
        self.type_aliases().count()
    }

    pub fn variable_count(&self) -> usize {
        self.variables().count()
    }

    pub fn unsupported_count(&self) -> usize {
        self.unsupported_items().count()
    }

    /// Retain only items that satisfy the given predicate.
    pub fn retain_items<F>(&mut self, pred: F)
    where
        F: Fn(&SourceItem) -> bool,
    {
        self.items.retain(&pred);
    }

    /// Merge another package's items, macros, diagnostics, and provenance into this one.
    ///
    /// Useful when scanning multiple headers into a single combined package.
    pub fn merge(&mut self, other: SourcePackage) {
        self.items.extend(other.items);
        self.macros.extend(other.macros);
        self.diagnostics.extend(other.diagnostics);
        self.provenance.extend(other.provenance);
        self.macro_provenance.extend(other.macro_provenance);
    }

    /// Return a summary of diagnostic counts by kind.
    pub fn diagnostics_count_by_kind(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for d in &self.diagnostics {
            let key = format!("{:?}", d.kind);
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    /// Summarize what degraded during extraction and how many items remain
    /// trustworthy enough for downstream consumers to inspect.
    pub fn extraction_status(&self) -> ExtractionStatus {
        let partial_diagnostic_count = self
            .diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.kind,
                    super::diagnostics::DiagnosticKind::DeclarationPartial
                )
            })
            .count();
        let unsupported_diagnostic_count = self
            .diagnostics
            .iter()
            .filter(|diag| {
                matches!(
                    diag.kind,
                    super::diagnostics::DiagnosticKind::DeclarationUnsupported
                )
            })
            .count();
        let parse_failure_count = self
            .diagnostics
            .iter()
            .filter(|diag| matches!(diag.kind, super::diagnostics::DiagnosticKind::ParseFailed))
            .count();
        let trustworthy_item_count = if parse_failure_count > 0 {
            0
        } else {
            self.item_count()
        };

        ExtractionStatus {
            item_count: self.item_count(),
            trustworthy_item_count,
            partial_diagnostic_count,
            unsupported_diagnostic_count,
            parse_failure_count,
        }
    }

    /// Render a concise human-readable summary of extraction trust and
    /// degradation state for logging, diagnostics, and release gates.
    pub fn extraction_status_message(&self) -> String {
        let status = self.extraction_status();
        format!(
            "{} trustworthy extracted items; {} partial diagnostics; {} unsupported diagnostics; {} parse failures",
            status.trustworthy_item_count,
            status.partial_diagnostic_count,
            status.unsupported_diagnostic_count,
            status.parse_failure_count,
        )
    }

    /// Build a typedef lookup table from this package's type aliases.
    fn typedef_table(&self) -> HashMap<&str, &SourceType> {
        self.type_aliases()
            .map(|ta| (ta.name.as_str(), &ta.target))
            .collect()
    }

    /// Transitively resolve a type, following `TypedefRef` chains to the
    /// underlying concrete type.
    ///
    /// Also recurses into Pointer pointees, Array elements, FunctionPointer
    /// return types and parameters, and Qualified inner types.
    ///
    /// Cycles are detected and left as `TypedefRef` to avoid infinite loops.
    pub fn resolve_type(&self, ty: &SourceType) -> SourceType {
        let table = self.typedef_table();
        resolve_type_rec(ty, &table, &mut HashSet::new())
    }

    /// Resolve all `TypedefRef` occurrences in every item in place.
    ///
    /// After calling this, function parameters, return types, record fields,
    /// variable types, and type alias targets will have their `TypedefRef`
    /// chains fully resolved to concrete types.
    pub fn resolve_all_typedefs(&mut self) {
        let table: HashMap<String, SourceType> = self
            .type_aliases()
            .map(|ta| (ta.name.clone(), ta.target.clone()))
            .collect();

        let ref_table: HashMap<&str, &SourceType> =
            table.iter().map(|(k, v)| (k.as_str(), v)).collect();

        for item in &mut self.items {
            match item {
                SourceItem::Function(f) => {
                    f.return_type =
                        resolve_type_rec(&f.return_type, &ref_table, &mut HashSet::new());
                    for param in &mut f.parameters {
                        param.ty = resolve_type_rec(&param.ty, &ref_table, &mut HashSet::new());
                    }
                }
                SourceItem::Record(r) => {
                    if let Some(fields) = &mut r.fields {
                        for field in fields {
                            field.ty = resolve_type_rec(&field.ty, &ref_table, &mut HashSet::new());
                        }
                    }
                }
                SourceItem::TypeAlias(ta) => {
                    ta.target = resolve_type_rec(&ta.target, &ref_table, &mut HashSet::new());
                }
                SourceItem::Variable(v) => {
                    v.ty = resolve_type_rec(&v.ty, &ref_table, &mut HashSet::new());
                }
                SourceItem::Enum(_) | SourceItem::Unsupported(_) => {}
            }
        }
    }
}

/// Recursively resolve a SourceType, following TypedefRef chains.
fn resolve_type_rec(
    ty: &SourceType,
    table: &HashMap<&str, &SourceType>,
    visited: &mut HashSet<String>,
) -> SourceType {
    match ty {
        SourceType::TypedefRef(name) => {
            if visited.contains(name.as_str()) {
                // Cycle detected — leave unresolved
                return ty.clone();
            }
            match table.get(name.as_str()) {
                Some(target) => {
                    visited.insert(name.clone());
                    let resolved = resolve_type_rec(target, table, visited);
                    visited.remove(name.as_str());
                    resolved
                }
                None => ty.clone(), // Unknown typedef — leave as-is
            }
        }
        SourceType::Pointer {
            pointee,
            qualifiers,
        } => SourceType::Pointer {
            pointee: Box::new(resolve_type_rec(pointee, table, visited)),
            qualifiers: *qualifiers,
        },
        SourceType::Array(elem, size) => {
            SourceType::Array(Box::new(resolve_type_rec(elem, table, visited)), *size)
        }
        SourceType::Qualified {
            ty: inner,
            qualifiers,
        } => {
            let resolved = resolve_type_rec(inner, table, visited);
            SourceType::qualified(resolved, *qualifiers)
        }
        SourceType::FunctionPointer {
            return_type,
            parameters,
            variadic,
        } => SourceType::FunctionPointer {
            return_type: Box::new(resolve_type_rec(return_type, table, visited)),
            parameters: parameters
                .iter()
                .map(|p| resolve_type_rec(p, table, visited))
                .collect(),
            variadic: *variadic,
        },
        // Primitives, RecordRef, EnumRef, Opaque — no resolution needed
        other => other.clone(),
    }
}

impl Default for SourcePackage {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing a SourcePackage programmatically.
///
/// Useful for downstream consumers (e.g., bic migration) that need to
/// build packages from non-extraction sources.
pub struct SourcePackageBuilder {
    package: SourcePackage,
}

impl Default for SourcePackageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SourcePackageBuilder {
    pub fn new() -> Self {
        Self {
            package: SourcePackage::new(),
        }
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.package.source_path = Some(path.into());
        self
    }

    pub fn target(mut self, target: SourceTarget) -> Self {
        self.package.target = target;
        self
    }

    pub fn inputs(mut self, inputs: SourceInputs) -> Self {
        self.package.inputs = inputs;
        self
    }

    pub fn item(mut self, item: SourceItem) -> Self {
        self.package.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = SourceItem>) -> Self {
        self.package.items.extend(items);
        self
    }

    pub fn macro_entry(mut self, m: super::macros::SourceMacro) -> Self {
        self.package.macros.push(m);
        self
    }

    pub fn diagnostic(mut self, d: super::diagnostics::SourceDiagnostic) -> Self {
        self.package.diagnostics.push(d);
        self
    }

    pub fn build(self) -> SourcePackage {
        self.package
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::diagnostics::{DiagnosticKind, Severity};
    use crate::ir::items::*;
    use crate::ir::macros::*;
    use crate::ir::types::SourceType;

    #[test]
    fn empty_package() {
        let pkg = SourcePackage::new();
        assert!(pkg.is_empty());
        assert!(!pkg.has_diagnostics());
        assert_eq!(pkg.item_count(), 0);
        assert_eq!(pkg.schema_version, SCHEMA_VERSION);
        assert!(pkg.source_path.is_none());
    }

    #[test]
    fn package_with_items() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::Function(SourceFunction {
            name: "malloc".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![SourceParameter {
                name: Some("size".into()),
                ty: SourceType::ULong,
            }],
            return_type: SourceType::ptr(SourceType::Void),
            variadic: false,
            source_offset: Some(10),
        }));
        pkg.items.push(SourceItem::Record(SourceRecord {
            kind: RecordKind::Struct,
            name: Some("point".into()),
            fields: Some(vec![SourceField {
                name: Some("x".into()),
                ty: SourceType::Int,
                bit_width: None,
            }]),
            source_offset: Some(20),
        }));
        pkg.items.push(SourceItem::Enum(SourceEnum {
            name: Some("color".into()),
            variants: vec![SourceEnumVariant {
                name: "RED".into(),
                value: Some(0),
            }],
            source_offset: None,
        }));
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "size_t".into(),
            target: SourceType::ULong,
            source_offset: None,
        }));
        pkg.items.push(SourceItem::Variable(SourceVariable {
            name: "errno".into(),
            ty: SourceType::Int,
            source_offset: None,
        }));

        assert!(!pkg.is_empty());
        assert_eq!(pkg.item_count(), 5);
        assert!(pkg.find_function("malloc").is_some());
        assert!(pkg.find_record("point").is_some());
        assert!(pkg.find_enum("color").is_some());
        assert!(pkg.find_type_alias("size_t").is_some());
        assert!(pkg.find_variable("errno").is_some());
        assert!(pkg.find_function("nonexistent").is_none());

        let counts = pkg.count_by_kind();
        assert_eq!(counts[&SourceItemKind::Function], 1);
        assert_eq!(counts[&SourceItemKind::Record], 1);
    }

    #[test]
    fn package_with_diagnostics() {
        let mut pkg = SourcePackage::new();
        pkg.diagnostics.push(SourceDiagnostic {
            kind: DiagnosticKind::ParseFailed,
            severity: Severity::Error,
            message: "bad token".into(),
            location: None,
            item_name: None,
        });
        assert!(pkg.has_diagnostics());
        assert!(!pkg.is_empty());
    }

    #[test]
    fn package_with_macros() {
        let mut pkg = SourcePackage::new();
        pkg.macros.push(SourceMacro {
            name: "SIZE".into(),
            body: "42".into(),
            form: MacroForm::ObjectLike,
            kind: MacroKind::Integer,
            value: Some(MacroValue::Integer(42)),
        });
        assert!(!pkg.is_empty());
    }

    #[test]
    fn json_roundtrip_full_package() {
        let mut pkg = SourcePackage::new();
        pkg.source_path = Some("test.h".into());
        pkg.items.push(SourceItem::Function(SourceFunction {
            name: "foo".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![],
            return_type: SourceType::Void,
            variadic: false,
            source_offset: None,
        }));
        pkg.macros.push(SourceMacro {
            name: "BAR".into(),
            body: "1".into(),
            form: MacroForm::ObjectLike,
            kind: MacroKind::Integer,
            value: Some(MacroValue::Integer(1)),
        });
        pkg.diagnostics.push(SourceDiagnostic::warning(
            DiagnosticKind::DeclarationPartial,
            "incomplete",
        ));

        let json = serde_json::to_string_pretty(&pkg).unwrap();
        let back: SourcePackage = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg, back);
    }

    #[test]
    fn deserialize_minimal_json() {
        // Minimal valid JSON (only required fields absent, defaults fill in)
        let json = r#"{"items":[],"diagnostics":[]}"#;
        let pkg: SourcePackage = serde_json::from_str(json).unwrap();
        assert_eq!(pkg.schema_version, SCHEMA_VERSION);
        assert!(pkg.items.is_empty());
    }

    #[test]
    fn deterministic_serialization() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::Variable(SourceVariable {
            name: "x".into(),
            ty: SourceType::Int,
            source_offset: None,
        }));
        let a = serde_json::to_string(&pkg).unwrap();
        let b = serde_json::to_string(&pkg).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn default_trait() {
        let pkg = SourcePackage::default();
        assert!(pkg.is_empty());
    }

    #[test]
    fn retain_items_filters() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::Function(SourceFunction {
            name: "foo".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![],
            return_type: SourceType::Void,
            variadic: false,
            source_offset: None,
        }));
        pkg.items.push(SourceItem::Variable(SourceVariable {
            name: "bar".into(),
            ty: SourceType::Int,
            source_offset: None,
        }));
        pkg.items.push(SourceItem::Function(SourceFunction {
            name: "baz".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![],
            return_type: SourceType::Int,
            variadic: false,
            source_offset: None,
        }));

        pkg.retain_items(|item| matches!(item, SourceItem::Function(_)));
        assert_eq!(pkg.item_count(), 2);
        assert_eq!(pkg.function_count(), 2);
        assert_eq!(pkg.variable_count(), 0);
    }

    #[test]
    fn merge_packages() {
        let mut pkg1 = SourcePackage::new();
        pkg1.items.push(SourceItem::Function(SourceFunction {
            name: "a".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![],
            return_type: SourceType::Void,
            variadic: false,
            source_offset: None,
        }));

        let mut pkg2 = SourcePackage::new();
        pkg2.items.push(SourceItem::Function(SourceFunction {
            name: "b".into(),
            calling_convention: CallingConvention::C,
            parameters: vec![],
            return_type: SourceType::Int,
            variadic: false,
            source_offset: None,
        }));
        pkg2.macros.push(SourceMacro {
            name: "M".into(),
            body: "1".into(),
            form: MacroForm::ObjectLike,
            kind: MacroKind::Integer,
            value: Some(MacroValue::Integer(1)),
        });

        pkg1.merge(pkg2);
        assert_eq!(pkg1.function_count(), 2);
        assert_eq!(pkg1.macros.len(), 1);
    }

    #[test]
    fn diagnostics_count_by_kind() {
        let mut pkg = SourcePackage::new();
        pkg.diagnostics.push(SourceDiagnostic::warning(
            DiagnosticKind::DeclarationPartial,
            "partial 1",
        ));
        pkg.diagnostics.push(SourceDiagnostic::warning(
            DiagnosticKind::DeclarationPartial,
            "partial 2",
        ));
        pkg.diagnostics.push(SourceDiagnostic::error(
            DiagnosticKind::ParseFailed,
            "parse failed",
        ));

        let counts = pkg.diagnostics_count_by_kind();
        assert_eq!(counts["DeclarationPartial"], 2);
        assert_eq!(counts["ParseFailed"], 1);
    }

    #[test]
    fn extraction_status_counts_partial_and_unsupported() {
        use super::super::diagnostics::{DiagnosticKind, SourceDiagnostic};

        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "ok_t".into(),
            target: SourceType::Int,
            source_offset: None,
        }));
        pkg.diagnostics.push(SourceDiagnostic::warning(
            DiagnosticKind::DeclarationPartial,
            "bitfield layout partially represented",
        ));
        pkg.diagnostics.push(SourceDiagnostic::warning(
            DiagnosticKind::DeclarationUnsupported,
            "block pointer not supported",
        ));

        let status = pkg.extraction_status();
        assert_eq!(status.item_count, 1);
        assert_eq!(status.trustworthy_item_count, 1);
        assert_eq!(status.partial_diagnostic_count, 1);
        assert_eq!(status.unsupported_diagnostic_count, 1);
        assert_eq!(status.parse_failure_count, 0);
        assert!(pkg
            .extraction_status_message()
            .contains("1 trustworthy extracted items"));
    }

    #[test]
    fn extraction_status_zeroes_trust_on_parse_failure() {
        use super::super::diagnostics::{DiagnosticKind, SourceDiagnostic};

        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "maybe_t".into(),
            target: SourceType::Int,
            source_offset: None,
        }));
        pkg.diagnostics.push(SourceDiagnostic::error(
            DiagnosticKind::ParseFailed,
            "parse error at line 10:7",
        ));

        let status = pkg.extraction_status();
        assert_eq!(status.item_count, 1);
        assert_eq!(status.trustworthy_item_count, 0);
        assert_eq!(status.parse_failure_count, 1);
        assert!(pkg.extraction_status_message().contains("1 parse failures"));
    }

    #[test]
    fn builder_constructs_package() {
        let pkg = SourcePackageBuilder::new()
            .source_path("test.h")
            .item(SourceItem::Function(SourceFunction {
                name: "foo".into(),
                calling_convention: CallingConvention::C,
                parameters: vec![],
                return_type: SourceType::Void,
                variadic: false,
                source_offset: None,
            }))
            .item(SourceItem::Variable(SourceVariable {
                name: "bar".into(),
                ty: SourceType::Int,
                source_offset: None,
            }))
            .build();

        assert_eq!(pkg.source_path.as_deref(), Some("test.h"));
        assert_eq!(pkg.function_count(), 1);
        assert_eq!(pkg.variable_count(), 1);
    }

    #[test]
    fn resolve_type_simple_typedef() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "my_int".into(),
            target: SourceType::Int,
            source_offset: None,
        }));

        let resolved = pkg.resolve_type(&SourceType::TypedefRef("my_int".into()));
        assert_eq!(resolved, SourceType::Int);
    }

    #[test]
    fn resolve_type_chain() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "uint32_t".into(),
            target: SourceType::UInt,
            source_offset: None,
        }));
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "my_uint".into(),
            target: SourceType::TypedefRef("uint32_t".into()),
            source_offset: None,
        }));
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "handle_t".into(),
            target: SourceType::TypedefRef("my_uint".into()),
            source_offset: None,
        }));

        // 3-deep chain: handle_t -> my_uint -> uint32_t -> UInt
        let resolved = pkg.resolve_type(&SourceType::TypedefRef("handle_t".into()));
        assert_eq!(resolved, SourceType::UInt);
    }

    #[test]
    fn resolve_type_through_pointer() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "size_t".into(),
            target: SourceType::ULong,
            source_offset: None,
        }));

        // const size_t * -> const unsigned long *
        let ty = SourceType::const_ptr(SourceType::TypedefRef("size_t".into()));
        let resolved = pkg.resolve_type(&ty);
        assert_eq!(resolved, SourceType::const_ptr(SourceType::ULong));
    }

    #[test]
    fn resolve_type_unknown_typedef_preserved() {
        let pkg = SourcePackage::new();
        // Unknown typedef stays as TypedefRef
        let resolved = pkg.resolve_type(&SourceType::TypedefRef("unknown_t".into()));
        assert_eq!(resolved, SourceType::TypedefRef("unknown_t".into()));
    }

    #[test]
    fn resolve_type_cycle_detection() {
        let mut pkg = SourcePackage::new();
        // Create a cycle: a -> b -> a
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "a".into(),
            target: SourceType::TypedefRef("b".into()),
            source_offset: None,
        }));
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "b".into(),
            target: SourceType::TypedefRef("a".into()),
            source_offset: None,
        }));

        // Should not infinite loop — cycle leaves TypedefRef in place
        let resolved = pkg.resolve_type(&SourceType::TypedefRef("a".into()));
        // Will be TypedefRef("a") since the cycle is detected
        assert!(matches!(resolved, SourceType::TypedefRef(_)));
    }

    #[test]
    fn resolve_type_in_function_pointer() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "my_int".into(),
            target: SourceType::Int,
            source_offset: None,
        }));

        let fp = SourceType::FunctionPointer {
            return_type: Box::new(SourceType::TypedefRef("my_int".into())),
            parameters: vec![SourceType::TypedefRef("my_int".into())],
            variadic: false,
        };
        let resolved = pkg.resolve_type(&fp);
        match resolved {
            SourceType::FunctionPointer {
                return_type,
                parameters,
                ..
            } => {
                assert_eq!(*return_type, SourceType::Int);
                assert_eq!(parameters[0], SourceType::Int);
            }
            other => panic!("expected FunctionPointer, got {:?}", other),
        }
    }

    #[test]
    fn resolve_type_in_array() {
        let mut pkg = SourcePackage::new();
        pkg.items.push(SourceItem::TypeAlias(SourceTypeAlias {
            name: "elem_t".into(),
            target: SourceType::Double,
            source_offset: None,
        }));

        let arr = SourceType::Array(Box::new(SourceType::TypedefRef("elem_t".into())), Some(10));
        let resolved = pkg.resolve_type(&arr);
        assert_eq!(
            resolved,
            SourceType::Array(Box::new(SourceType::Double), Some(10))
        );
    }

    #[test]
    fn resolve_all_typedefs_full() {
        use crate::extract;

        let src = r#"
typedef unsigned int uint32_t;
typedef uint32_t my_uint;

struct config {
    my_uint flags;
    const my_uint *data;
};

my_uint get_value(const my_uint *ptr);
extern my_uint global;
"#;
        let mut pkg = extract::extract_from_source(src).unwrap();

        // Before resolution: types reference "my_uint" and "uint32_t"
        let f = pkg.find_function("get_value").unwrap();
        assert_eq!(f.return_type, SourceType::TypedefRef("my_uint".into()));

        // Resolve
        pkg.resolve_all_typedefs();

        // After resolution: all TypedefRef chains resolved to UInt
        let f = pkg.find_function("get_value").unwrap();
        assert_eq!(f.return_type, SourceType::UInt);
        assert_eq!(f.parameters[0].ty, SourceType::const_ptr(SourceType::UInt));

        let v = pkg.find_variable("global").unwrap();
        assert_eq!(v.ty, SourceType::UInt);

        let config = pkg.find_record("config").unwrap();
        let fields = config.fields.as_ref().unwrap();
        assert_eq!(fields[0].ty, SourceType::UInt);
        assert_eq!(fields[1].ty, SourceType::const_ptr(SourceType::UInt));

        // Type aliases themselves are also resolved
        let my_uint = pkg.find_type_alias("my_uint").unwrap();
        assert_eq!(my_uint.target, SourceType::UInt);
    }

    #[test]
    fn resolve_all_typedefs_preserves_record_and_enum_refs() {
        use crate::extract;

        let src = r#"
struct point { int x; int y; };
enum color { RED, GREEN, BLUE };
typedef struct point point_t;
void draw(point_t p, enum color c);
"#;
        let mut pkg = extract::extract_from_source(src).unwrap();
        pkg.resolve_all_typedefs();

        // point_t -> RecordRef("point"), not further resolved
        let alias = pkg.find_type_alias("point_t").unwrap();
        assert_eq!(alias.target, SourceType::RecordRef("point".into()));

        // draw's first param was point_t -> resolves to RecordRef("point")
        let draw = pkg.find_function("draw").unwrap();
        assert_eq!(draw.parameters[0].ty, SourceType::RecordRef("point".into()));
        assert_eq!(draw.parameters[1].ty, SourceType::EnumRef("color".into()));
    }
}
