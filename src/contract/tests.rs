//! Contract golden-vector and adversarial codec tests.

use serde_json::{json, Value};

use super::{ids::canonical_tokens_bytes, *};

fn scalar(storage_bits: u16, alignment_bits: u16) -> ScalarLayout {
    ScalarLayout {
        storage_bits,
        alignment_bits,
    }
}

fn integer(storage_bits: u16, alignment_bits: u16, signedness: Signedness) -> IntegerLayout {
    IntegerLayout {
        scalar: scalar(storage_bits, alignment_bits),
        signedness,
        representation: SignedIntegerRepresentation::TwosComplement,
    }
}

fn target() -> TargetSpec {
    target_with_standard(LanguageStandard::C17)
}

fn target_with_standard(language_standard: LanguageStandard) -> TargetSpec {
    let triple = "x86_64-unknown-linux-gnu";
    TargetSpec::try_new(TargetSpecParts {
        triple: triple.to_owned(),
        architecture: Architecture::X86_64,
        vendor: Vendor::try_new("unknown").unwrap(),
        operating_system: OperatingSystem::Linux,
        environment: Environment::Gnu,
        object_format: ObjectFormat::Elf,
        endian: Endian::Little,
        pointer_width: 64,
        c_data_model: CDataModel {
            class: CDataModelClass::LP64,
            char_bit: 8,
            char_signedness: CharSignedness::Signed,
            signed_integer_representation: SignedIntegerRepresentation::TwosComplement,
            bool_layout: scalar(8, 8),
            char_layout: scalar(8, 8),
            short_layout: scalar(16, 16),
            int_layout: scalar(32, 32),
            long_layout: scalar(64, 64),
            long_long_layout: scalar(64, 64),
            int128_layout: Some(scalar(128, 128)),
            pointer_layout: scalar(64, 64),
            float_layout: FloatingLayout {
                scalar: scalar(32, 32),
                format: FloatingFormat::IeeeBinary32,
            },
            double_layout: FloatingLayout {
                scalar: scalar(64, 64),
                format: FloatingFormat::IeeeBinary64,
            },
            long_double_layout: FloatingLayout {
                scalar: scalar(128, 128),
                format: FloatingFormat::X87Extended80,
            },
            wchar_layout: integer(32, 32, Signedness::Signed),
            size_t_layout: integer(64, 64, Signedness::Unsigned),
            ptrdiff_t_layout: integer(64, 64, Signedness::Signed),
        },
        language_standard,
        extension_profile: ExtensionProfile::new(
            ExtensionFamily::Gnu,
            [ExtensionId::try_new("attributes").unwrap()],
        ),
        compiler: CompilerIdentity::try_new(
            CompilerFamily::Gcc,
            "toolchains/gcc/bin/gcc",
            ContentFingerprint::from_content(b"preservation gcc executable"),
            ContentFingerprint::from_content(b"gcc 13.2.0 preservation output"),
            triple,
            "13.2.0",
        )
        .unwrap(),
        sysroot: Some(
            SysrootIdentity::try_new(
                "toolchains/gcc/sysroot",
                ContentFingerprint::from_content(b"preservation sysroot"),
            )
            .unwrap(),
        ),
        abi_flags: vec![NormalizedCompilerArg::try_new("-m64").unwrap()],
    })
    .unwrap()
}

fn source_name(name: &str) -> SourceName {
    SourceName {
        normalized: name.to_owned(),
        original: name.to_owned(),
    }
}

fn provenance() -> SourceProvenance {
    SourceProvenance {
        origin: SourceOrigin::Entry,
        include_chain: Vec::new(),
        macro_expansions: Vec::new(),
    }
}

fn range(file: FileId, start: u64, end: u64) -> SourceRange {
    SourceRange { file, start, end }
}

fn supported_type(kind: CTypeKind) -> CType {
    CType {
        qualifiers: TypeQualifiers::NONE,
        nullability: Nullability::Unspecified,
        kind,
        support: SupportStatus::Supported,
    }
}

fn named_id(namespace: EntityNamespace, name: &str) -> DeclarationId {
    DeclarationId::from_entity(
        EntityId::named(namespace, EntityScope::TranslationUnit, name).unwrap(),
    )
}

#[derive(Clone, Copy)]
struct FixtureSource<'a> {
    file: FileId,
    text: &'a str,
}

impl FixtureSource<'_> {
    fn unique_range(self, spelling: &str) -> SourceRange {
        let mut matches = self.text.match_indices(spelling);
        let (start, _) = matches
            .next()
            .unwrap_or_else(|| panic!("fixture spelling not found: {spelling:?}"));
        assert!(
            matches.next().is_none(),
            "fixture spelling is not unique: {spelling:?}"
        );
        range(self.file, start as u64, (start + spelling.len()) as u64)
    }

    fn subrange(self, outer: SourceRange, spelling: &str) -> SourceRange {
        let outer_text = self.slice(outer);
        let mut matches = outer_text.match_indices(spelling);
        let (relative_start, _) = matches
            .next()
            .unwrap_or_else(|| panic!("nested fixture spelling not found: {spelling:?}"));
        assert!(
            matches.next().is_none(),
            "nested fixture spelling is not unique: {spelling:?}"
        );
        let start = outer.start + relative_start as u64;
        range(self.file, start, start + spelling.len() as u64)
    }

    fn slice(self, source_range: SourceRange) -> &'static str {
        assert_eq!(source_range.file, self.file);
        &corpus::PRESERVATION_HEADER[source_range.start as usize..source_range.end as usize]
    }
}

#[allow(clippy::too_many_arguments)]
fn occurrence(
    id: DeclarationId,
    source: FixtureSource<'_>,
    declaration_spelling: &str,
    name: &str,
    normalized_tokens: &[&str],
    storage: StorageClass,
    is_definition: bool,
    attributes: Vec<SourceAttribute>,
) -> DeclarationOccurrence {
    let occurrence_range = source.unique_range(declaration_spelling);
    let normalized_tokens: Vec<_> = normalized_tokens
        .iter()
        .map(|token| (*token).to_owned())
        .collect();
    DeclarationOccurrence {
        id: OccurrenceId::derive(
            id,
            source.file,
            &canonical_tokens_bytes(&normalized_tokens),
            0,
        ),
        range: occurrence_range,
        name_range: Some(source.subrange(occurrence_range, name)),
        spelling: source.slice(occurrence_range).to_owned(),
        normalized_tokens,
        duplicate_ordinal: 0,
        storage,
        is_definition,
        attributes,
        provenance: provenance(),
    }
}

fn declaration(
    id: DeclarationId,
    namespace: EntityNamespace,
    name: &str,
    linkage: Linkage,
    occurrence: DeclarationOccurrence,
    support: SupportStatus,
    kind: SourceDeclarationKind,
) -> SourceDeclaration {
    SourceDeclaration {
        id,
        identity: DeclarationIdentity::Named {
            namespace,
            scope: EntityScope::TranslationUnit,
            normalized_name: name.to_owned(),
        },
        name: Some(source_name(name)),
        linkage,
        visibility: Visibility::Unspecified,
        occurrences: vec![occurrence],
        support,
        kind,
    }
}

fn modeled_attribute(
    source: FixtureSource<'_>,
    name: &str,
    spelling: &str,
    arguments: &[&str],
) -> SourceAttribute {
    SourceAttribute {
        namespace: Some("gnu".to_owned()),
        name: name.to_owned(),
        arguments: arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect(),
        spelling: spelling.to_owned(),
        range: source.unique_range(spelling),
        disposition: AttributeDisposition::Modeled,
    }
}

fn fixture_input(partial: bool) -> SourcePackageInput {
    const OPAQUE_DECL: &str = "struct parc_opaque;";
    const ALIAS_DECL: &str = "typedef const volatile struct parc_opaque *parc_handle;";
    const PACKET_DECL: &str = "struct parc_packet {\n    int value;\n};";
    const MODE_DECL: &str = "enum parc_mode {\n    PARC_MODE_FAST = 7\n};";
    const OPEN_DECL: &str = "parc_handle parc_open(parc_handle restrict handle)\n    __attribute__((nonnull(1), ms_abi));";
    const MISSING_DECL: &str = "struct parc_opaque *parc_missing(struct parc_opaque *handle);";
    const EXTENDED_DECL: &str =
        "__float128 __attribute__((preserve_most)) parc_extended(__int128 wide);";

    let file_path = "include/preservation.h";
    let file = FileId::from_logical_path(file_path).unwrap();
    let source = FixtureSource {
        file,
        text: corpus::PRESERVATION_HEADER,
    };
    let opaque_id = named_id(EntityNamespace::Tag, "parc_opaque");
    let alias_id = named_id(EntityNamespace::Ordinary, "parc_handle");
    let packet_id = named_id(EntityNamespace::Tag, "parc_packet");
    let mode_id = named_id(EntityNamespace::Tag, "parc_mode");
    let open_id = named_id(EntityNamespace::Ordinary, "parc_open");
    let missing_id = named_id(EntityNamespace::Ordinary, "parc_missing");

    let opaque_occurrence = occurrence(
        opaque_id,
        source,
        OPAQUE_DECL,
        "parc_opaque",
        &["struct", "parc_opaque", ";"],
        StorageClass::None,
        false,
        Vec::new(),
    );
    let opaque = declaration(
        opaque_id,
        EntityNamespace::Tag,
        "parc_opaque",
        Linkage::None,
        opaque_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::Record(SourceRecord {
            kind: RecordKind::Struct,
            completeness: RecordCompleteness::Incomplete,
            fields: Vec::new(),
        }),
    );

    let alias_occurrence = occurrence(
        alias_id,
        source,
        ALIAS_DECL,
        "parc_handle",
        &[
            "typedef",
            "const",
            "volatile",
            "struct",
            "parc_opaque",
            "*",
            "parc_handle",
            ";",
        ],
        StorageClass::Typedef,
        true,
        Vec::new(),
    );
    let alias = declaration(
        alias_id,
        EntityNamespace::Ordinary,
        "parc_handle",
        Linkage::None,
        alias_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::TypeAlias(SourceTypeAlias {
            target: CType {
                qualifiers: TypeQualifiers::NONE,
                nullability: Nullability::Unspecified,
                kind: CTypeKind::Pointer(Box::new(CType {
                    qualifiers: TypeQualifiers {
                        is_const: true,
                        is_volatile: true,
                        is_restrict: false,
                        is_atomic: false,
                    },
                    nullability: Nullability::Unspecified,
                    kind: CTypeKind::RecordRef(opaque_id),
                    support: SupportStatus::Supported,
                })),
                support: SupportStatus::Supported,
            },
        }),
    );

    let field_id = ChildId::named(packet_id, ChildRole::Field, "value").unwrap();
    let packet_occurrence = occurrence(
        packet_id,
        source,
        PACKET_DECL,
        "parc_packet",
        &["struct", "parc_packet", "{", "int", "value", ";", "}", ";"],
        StorageClass::None,
        true,
        Vec::new(),
    );
    let packet = declaration(
        packet_id,
        EntityNamespace::Tag,
        "parc_packet",
        Linkage::None,
        packet_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::Record(SourceRecord {
            kind: RecordKind::Struct,
            completeness: RecordCompleteness::Complete,
            fields: vec![SourceField {
                id: field_id,
                name: Some(source_name("value")),
                ty: supported_type(CTypeKind::Integer(CIntegerType::Int {
                    signedness: Signedness::Signed,
                })),
                bit_width: None,
                range: source.unique_range("int value"),
                provenance: provenance(),
                attributes: Vec::new(),
                support: SupportStatus::Supported,
                identity_tokens: vec!["int".to_owned(), "value".to_owned()],
                duplicate_ordinal: 0,
            }],
        }),
    );

    let variant_id = ChildId::named(mode_id, ChildRole::EnumVariant, "PARC_MODE_FAST").unwrap();
    let mode_occurrence = occurrence(
        mode_id,
        source,
        MODE_DECL,
        "parc_mode",
        &[
            "enum",
            "parc_mode",
            "{",
            "PARC_MODE_FAST",
            "=",
            "7",
            "}",
            ";",
        ],
        StorageClass::None,
        true,
        Vec::new(),
    );
    let mode = declaration(
        mode_id,
        EntityNamespace::Tag,
        "parc_mode",
        Linkage::None,
        mode_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::Enum(SourceEnum {
            explicit_underlying_type: None,
            variants: vec![SourceEnumVariant {
                id: variant_id,
                name: source_name("PARC_MODE_FAST"),
                value: EnumValue::Evaluated {
                    value: ExactInteger::signed(7),
                },
                range: source.unique_range("PARC_MODE_FAST = 7"),
                provenance: provenance(),
                attributes: Vec::new(),
                support: SupportStatus::Supported,
                identity_tokens: vec!["PARC_MODE_FAST".to_owned(), "=".to_owned(), "7".to_owned()],
                duplicate_ordinal: 0,
            }],
        }),
    );

    let open_attributes = vec![
        modeled_attribute(source, "nonnull", "nonnull(1)", &["1"]),
        modeled_attribute(source, "ms_abi", "ms_abi", &[]),
    ];
    let open_occurrence = occurrence(
        open_id,
        source,
        OPEN_DECL,
        "parc_open",
        &[
            "parc_handle",
            "parc_open",
            "(",
            "parc_handle",
            "restrict",
            "handle",
            ")",
            "__attribute__",
            "(",
            "(",
            "nonnull",
            "(",
            "1",
            ")",
            ",",
            "ms_abi",
            ")",
            ")",
            ";",
        ],
        StorageClass::None,
        false,
        open_attributes,
    );
    let open = declaration(
        open_id,
        EntityNamespace::Ordinary,
        "parc_open",
        Linkage::External,
        open_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::Function(SourceFunction {
            link_name: "parc_open".to_owned(),
            return_type: supported_type(CTypeKind::AliasRef(alias_id)),
            parameters: vec![SourceParameter {
                id: ChildId::parameter(open_id, 0),
                ordinal: 0,
                name: Some(source_name("handle")),
                ty: CType {
                    qualifiers: TypeQualifiers {
                        is_const: false,
                        is_volatile: false,
                        is_restrict: true,
                        is_atomic: false,
                    },
                    nullability: Nullability::Nonnull,
                    kind: CTypeKind::AliasRef(alias_id),
                    support: SupportStatus::Supported,
                },
                range: source.unique_range("parc_handle restrict handle"),
                provenance: provenance(),
                attributes: Vec::new(),
                support: SupportStatus::Supported,
            }],
            prototype: FunctionPrototype::Prototyped { variadic: false },
            calling_convention: CallingConvention::Win64,
        }),
    );

    let missing_occurrence = occurrence(
        missing_id,
        source,
        MISSING_DECL,
        "parc_missing",
        &[
            "struct",
            "parc_opaque",
            "*",
            "parc_missing",
            "(",
            "struct",
            "parc_opaque",
            "*",
            "handle",
            ")",
            ";",
        ],
        StorageClass::None,
        false,
        Vec::new(),
    );
    let missing = declaration(
        missing_id,
        EntityNamespace::Ordinary,
        "parc_missing",
        Linkage::External,
        missing_occurrence,
        SupportStatus::Supported,
        SourceDeclarationKind::Function(SourceFunction {
            link_name: "parc_missing".to_owned(),
            return_type: supported_type(CTypeKind::Pointer(Box::new(supported_type(
                CTypeKind::RecordRef(opaque_id),
            )))),
            parameters: vec![SourceParameter {
                id: ChildId::parameter(missing_id, 0),
                ordinal: 0,
                name: Some(source_name("handle")),
                ty: supported_type(CTypeKind::Pointer(Box::new(supported_type(
                    CTypeKind::RecordRef(opaque_id),
                )))),
                range: source.unique_range("struct parc_opaque *handle"),
                provenance: provenance(),
                attributes: Vec::new(),
                support: SupportStatus::Supported,
            }],
            prototype: FunctionPrototype::Prototyped { variadic: false },
            calling_convention: CallingConvention::C,
        }),
    );

    let mut declarations = vec![opaque, alias, packet, mode, open, missing];
    let extended_id = named_id(EntityNamespace::Ordinary, "parc_extended");
    if partial {
        let code = DiagnosticCode::new("PARC-E4100").unwrap();
        let reason = "extended scalar ABI projection is unsupported".to_owned();
        let unsupported_attribute = SourceAttribute {
            namespace: Some("gnu".to_owned()),
            name: "preserve_most".to_owned(),
            arguments: Vec::new(),
            spelling: "preserve_most".to_owned(),
            range: source.unique_range("preserve_most"),
            disposition: AttributeDisposition::UnsupportedAbiRelevant,
        };
        let extended_occurrence = occurrence(
            extended_id,
            source,
            EXTENDED_DECL,
            "parc_extended",
            &[
                "__float128",
                "__attribute__",
                "(",
                "(",
                "preserve_most",
                ")",
                ")",
                "parc_extended",
                "(",
                "__int128",
                "wide",
                ")",
                ";",
            ],
            StorageClass::None,
            false,
            vec![unsupported_attribute],
        );
        declarations.push(declaration(
            extended_id,
            EntityNamespace::Ordinary,
            "parc_extended",
            Linkage::External,
            extended_occurrence,
            SupportStatus::Partial {
                code: code.clone(),
                reason: reason.clone(),
            },
            SourceDeclarationKind::Function(SourceFunction {
                link_name: "parc_extended".to_owned(),
                return_type: CType {
                    qualifiers: TypeQualifiers::NONE,
                    nullability: Nullability::Unspecified,
                    kind: CTypeKind::Floating(CFloatingType::Float128),
                    support: SupportStatus::Unsupported {
                        code: code.clone(),
                        reason: reason.clone(),
                    },
                },
                parameters: vec![SourceParameter {
                    id: ChildId::parameter(extended_id, 0),
                    ordinal: 0,
                    name: Some(source_name("wide")),
                    ty: CType {
                        qualifiers: TypeQualifiers::NONE,
                        nullability: Nullability::Unspecified,
                        kind: CTypeKind::Integer(CIntegerType::Int128 {
                            signedness: Signedness::Signed,
                        }),
                        support: SupportStatus::Unsupported {
                            code: code.clone(),
                            reason: reason.clone(),
                        },
                    },
                    range: source.unique_range("__int128 wide"),
                    provenance: provenance(),
                    attributes: Vec::new(),
                    support: SupportStatus::Unsupported {
                        code: code.clone(),
                        reason: reason.clone(),
                    },
                }],
                prototype: FunctionPrototype::Prototyped { variadic: false },
                calling_convention: CallingConvention::Unsupported {
                    spelling: "__attribute__((preserve_most))".to_owned(),
                },
            }),
        ));
    }
    declarations.sort_by_key(|declaration| declaration.id);

    let macro_id = MacroId::named(file, "PARC_ABI_LEVEL").unwrap();
    let macro_tokens = vec!["7".to_owned()];
    let macros = vec![SourceMacro {
        id: macro_id,
        identity_file: file,
        name: "PARC_ABI_LEVEL".to_owned(),
        form: MacroForm::ObjectLike,
        category: MacroCategory::AbiAffecting,
        body: "7".to_owned(),
        normalized_tokens: macro_tokens.clone(),
        value: Some(MacroValue::Integer {
            value: ExactInteger::signed(7),
        }),
        occurrences: vec![MacroOccurrence {
            id: OccurrenceId::derive_macro(
                macro_id,
                file,
                &canonical_tokens_bytes(&macro_tokens),
                0,
            ),
            range: source.unique_range("#define PARC_ABI_LEVEL 7"),
            normalized_tokens: macro_tokens,
            duplicate_ordinal: 0,
            provenance: provenance(),
        }],
        support: SupportStatus::Supported,
    }];

    let target = target();
    let (diagnostics, completeness) = if partial {
        let code = DiagnosticCode::new("PARC-E4100").unwrap();
        let message = "extended scalar ABI projection is unsupported".to_owned();
        let diagnostic_range = Some(source.unique_range("__attribute__((preserve_most))"));
        (
            vec![SourceDiagnostic {
                code: code.clone(),
                stage: DiagnosticStage::Extract,
                severity: Severity::Error,
                completeness_impact: DiagnosticCompletenessImpact::ForcesPartial,
                message: message.clone(),
                range: diagnostic_range,
                related: Vec::new(),
                declaration: Some(extended_id),
                target: target.fingerprint(),
            }],
            Completeness::Partial {
                reasons: vec![CompletenessReason {
                    code,
                    message,
                    range: diagnostic_range,
                }],
            },
        )
    } else {
        (Vec::new(), Completeness::Complete)
    };

    let source_bytes = corpus::PRESERVATION_HEADER.as_bytes();
    let mut line_starts = vec![0];
    line_starts.extend(
        source_bytes
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index as u64 + 1))
            .filter(|start| *start < source_bytes.len() as u64),
    );
    let mut define_events = vec![DefineEvent::Define {
        name: "PRESERVATION_MODE".to_owned(),
        value: Some("1".to_owned()),
    }];
    if partial {
        define_events.push(DefineEvent::Define {
            name: "PRESERVATION_PARTIAL".to_owned(),
            value: Some("1".to_owned()),
        });
    }

    SourcePackageInput {
        target,
        files: vec![SourceFile {
            id: file,
            logical_path: file_path.to_owned(),
            role: SourceFileRole::Entry,
            content: ContentFingerprint::from_content(source_bytes),
            byte_len: source_bytes.len() as u64,
            line_starts,
        }],
        inputs: EffectiveSourceInputs {
            entry_files: vec![file],
            include_search: vec![IncludeSearchEntry {
                logical_path: "include".to_owned(),
                kind: IncludeSearchKind::User,
                content: Some(ContentFingerprint::from_content(
                    b"preservation include tree",
                )),
            }],
            define_events,
            forced_includes: Vec::new(),
            preprocessor: PreprocessorIdentity::Builtin {
                implementation_version: "parc-preservation-v2".to_owned(),
            },
            environment: EnvironmentInputs::Hermetic,
            path_mapping_fingerprint: ContentFingerprint::from_content(
                b"logical preservation path map",
            ),
        },
        declarations,
        macros,
        diagnostics,
        completeness,
    }
}

fn complete_package() -> SourcePackage {
    SourcePackage::try_new(fixture_input(false)).unwrap()
}

fn partial_package() -> SourcePackage {
    SourcePackage::try_new(fixture_input(true)).unwrap()
}

#[test]
fn codec_round_trip_is_byte_canonical() {
    for package in [complete_package(), partial_package()] {
        let encoded = encode_source_package(&package).unwrap();
        let decoded = decode_source_package(&encoded).unwrap();
        assert_eq!(decoded, package);
        assert_eq!(encode_source_package(&decoded).unwrap(), encoded);
        assert_eq!(decoded.target_fingerprint(), package.target_fingerprint());
        assert_eq!(decoded.fingerprint(), package.fingerprint());
    }
}

#[test]
fn decoder_rejects_missing_zero_and_future_schema_versions() {
    let bytes = encode_source_package(&complete_package()).unwrap();
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    value["schema"].as_object_mut().unwrap().remove("version");
    assert!(decode_source_package(&serde_json::to_vec(&value).unwrap()).is_err());

    for version in [0, 1, 3, u32::MAX] {
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["schema"]["version"] = json!(version);
        assert!(matches!(
            decode_source_package(&serde_json::to_vec(&value).unwrap()),
            Err(DecodeError::SchemaVersion { found }) if found == version
        ));
    }
}

#[test]
fn decoder_rejects_unknown_fields_throughout_nested_wire_shapes() {
    let bytes = encode_source_package(&complete_package()).unwrap();
    let base: Value = serde_json::from_slice(&bytes).unwrap();
    let mut mutations = Vec::new();

    let mut envelope = base.clone();
    envelope["unknown"] = json!(true);
    mutations.push(("envelope", envelope));

    let mut target = base.clone();
    target["payload"]["target"]["spec"]["unknown"] = json!(true);
    mutations.push(("target", target));

    let mut input_event = base.clone();
    input_event["payload"]["inputs"]["define_events"][0]["unknown"] = json!(true);
    mutations.push(("input_event", input_event));

    let mut completeness = base.clone();
    completeness["payload"]["completeness"]["unknown"] = json!(true);
    mutations.push(("completeness", completeness));

    let declaration_index = base["payload"]["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .position(|declaration| declaration["kind"]["kind"] == "function")
        .unwrap();

    let mut declaration_kind = base.clone();
    declaration_kind["payload"]["declarations"][declaration_index]["kind"]["unknown"] = json!(true);
    mutations.push(("declaration_kind", declaration_kind));

    let mut type_kind = base.clone();
    type_kind["payload"]["declarations"][declaration_index]["kind"]["value"]["return_type"]
        ["kind"]["unknown"] = json!(true);
    mutations.push(("type_kind", type_kind));

    let mut support = base.clone();
    support["payload"]["declarations"][declaration_index]["support"]["unknown"] = json!(true);
    mutations.push(("support", support));

    let mut target_enum = base.clone();
    target_enum["payload"]["target"]["spec"]["architecture"] = json!({
        "value": "x86_64",
        "unknown": true
    });
    mutations.push(("target_enum", target_enum));

    let mut environment = base.clone();
    environment["payload"]["inputs"]["environment"]["unknown"] = json!(true);
    mutations.push(("environment", environment));

    let mut preprocessor = base.clone();
    preprocessor["payload"]["inputs"]["preprocessor"]["unknown"] = json!(true);
    mutations.push(("preprocessor", preprocessor));

    let mut scope = base.clone();
    scope["payload"]["declarations"][declaration_index]["identity"]["scope"]["unknown"] =
        json!(true);
    mutations.push(("entity_scope", scope));

    let mut prototype = base.clone();
    prototype["payload"]["declarations"][declaration_index]["kind"]["value"]["prototype"]
        ["unknown"] = json!(true);
    mutations.push(("function_prototype", prototype));

    let mut calling_convention = base.clone();
    calling_convention["payload"]["declarations"][declaration_index]["kind"]["value"]
        ["calling_convention"]["unknown"] = json!(true);
    mutations.push(("calling_convention", calling_convention));

    let mut target_class = base.clone();
    target_class["payload"]["target"]["spec"]["c_data_model"]["class"]["unknown"] = json!(true);
    mutations.push(("target_data_model_class", target_class));

    let mut target_float_format = base.clone();
    target_float_format["payload"]["target"]["spec"]["c_data_model"]["float_layout"]["format"]
        ["unknown"] = json!(true);
    mutations.push(("target_float_format", target_float_format));

    let record_index = base["payload"]["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .position(|declaration| {
            declaration["kind"]["kind"] == "record"
                && declaration["kind"]["value"]["completeness"] == "complete"
        })
        .unwrap();
    let mut integer_rank = base.clone();
    integer_rank["payload"]["declarations"][record_index]["kind"]["value"]["fields"][0]["ty"]
        ["kind"]["value"]["unknown"] = json!(true);
    mutations.push(("integer_rank", integer_rank));

    let mut array_bound = base.clone();
    let element = array_bound["payload"]["declarations"][record_index]["kind"]["value"]["fields"]
        [0]["ty"]
        .clone();
    array_bound["payload"]["declarations"][record_index]["kind"]["value"]["fields"][0]["ty"]
        ["kind"] = json!({
        "kind": "array",
        "value": {
            "element": element,
            "bound": {"kind": "incomplete", "unknown": true},
            "parameter_qualifiers": {
                "is_const": false,
                "is_volatile": false,
                "is_restrict": false,
                "is_atomic": false
            }
        }
    });
    mutations.push(("array_bound", array_bound));

    let mut macro_value = base.clone();
    macro_value["payload"]["macros"][0]["value"]["unknown"] = json!(true);
    mutations.push(("macro_value", macro_value));

    let enum_index = base["payload"]["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .position(|declaration| declaration["kind"]["kind"] == "enum")
        .unwrap();
    let mut enum_value = base.clone();
    enum_value["payload"]["declarations"][enum_index]["kind"]["value"]["variants"][0]["value"]
        ["unknown"] = json!(true);
    mutations.push(("enum_value", enum_value));

    for (name, mutation) in mutations {
        assert!(
            decode_source_package(&serde_json::to_vec(&mutation).unwrap()).is_err(),
            "nested unknown-field mutation was accepted: {name}"
        );
    }
}

#[test]
fn complete_wrapper_accepts_direct_and_typedef_mediated_opaque_handles() {
    let package = complete_package();
    let opaque = named_id(EntityNamespace::Tag, "parc_opaque");
    let alias = named_id(EntityNamespace::Ordinary, "parc_handle");
    let function = named_id(EntityNamespace::Ordinary, "parc_open");
    let direct_function = named_id(EntityNamespace::Ordinary, "parc_missing");
    let by_value_record = named_id(EntityNamespace::Tag, "parc_packet");

    let typedef_mediated = package
        .clone()
        .into_complete(&Selection::only([function]).unwrap())
        .unwrap();
    assert_eq!(
        closure_requirement(&typedef_mediated, alias),
        Some(ClosureRequirement::Definition)
    );
    assert_eq!(
        closure_requirement(&typedef_mediated, opaque),
        Some(ClosureRequirement::Opaque)
    );

    let direct = package
        .clone()
        .into_complete(&Selection::only([direct_function]).unwrap())
        .unwrap();
    assert_eq!(
        closure_requirement(&direct, opaque),
        Some(ClosureRequirement::Opaque)
    );

    let by_value = package
        .clone()
        .into_complete(&Selection::only([by_value_record]).unwrap())
        .unwrap();
    assert_eq!(
        closure_requirement(&by_value, by_value_record),
        Some(ClosureRequirement::Definition)
    );

    package
        .clone()
        .into_complete(&Selection::opaque([opaque]).unwrap())
        .unwrap();
    package.into_complete(&Selection::all_supported()).unwrap();
}

fn closure_requirement(
    complete: &CompleteSourcePackage,
    declaration: DeclarationId,
) -> Option<ClosureRequirement> {
    complete
        .declaration_closure()
        .iter()
        .find(|entry| entry.declaration() == declaration)
        .map(DeclarationClosureEntry::requirement)
}

#[test]
fn opaque_selection_rejects_non_record_roots() {
    let function = named_id(EntityNamespace::Ordinary, "parc_open");
    let error = complete_package()
        .into_complete(&Selection::opaque([function]).unwrap())
        .unwrap_err();
    assert!(matches!(
        error.blockers(),
        [CompletionBlocker::OpaqueRootIsNotRecord { id }] if *id == function
    ));
}

#[test]
fn partial_package_cannot_be_forged_into_complete() {
    let error = partial_package()
        .into_complete(&Selection::all_supported())
        .unwrap_err();
    assert!(error
        .blockers()
        .iter()
        .any(|blocker| matches!(blocker, CompletionBlocker::PackageIncomplete { .. })));
}

#[test]
fn retain_keeps_the_exact_transitive_reference_closure() {
    let open = named_id(EntityNamespace::Ordinary, "parc_open");
    let alias = named_id(EntityNamespace::Ordinary, "parc_handle");
    let opaque = named_id(EntityNamespace::Tag, "parc_opaque");
    let extended = named_id(EntityNamespace::Ordinary, "parc_extended");

    let retained = partial_package()
        .retain(&Selection::only([open]).unwrap())
        .unwrap();
    let ids = retained
        .declarations()
        .iter()
        .map(|declaration| declaration.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids, [open, alias, opaque].into_iter().collect());
    assert_eq!(retained.completeness(), &Completeness::Complete);
    assert!(retained.diagnostics().is_empty());
    assert!(retained.declaration(extended).is_none());
    assert_eq!(retained.files(), partial_package().files());
    assert_eq!(retained.macros(), partial_package().macros());

    retained
        .into_complete(&Selection::only([open]).unwrap())
        .unwrap();
}

#[test]
fn retain_validates_explicit_and_opaque_roots() {
    let missing = named_id(EntityNamespace::Ordinary, "not_present");
    assert!(matches!(
        complete_package().retain(&Selection::only([missing]).unwrap()),
        Err(ComposeError::MissingDeclaration { id }) if id == missing
    ));

    let function = named_id(EntityNamespace::Ordinary, "parc_open");
    assert!(matches!(
        complete_package().retain(&Selection::opaque([function]).unwrap()),
        Err(ComposeError::OpaqueRootIsNotRecord { id }) if id == function
    ));
}

#[test]
fn merge_unions_compatible_entry_sets_and_reference_closed_fragments() {
    let package = complete_package();
    assert_eq!(package.clone().merge(package.clone()).unwrap(), package);

    let open = named_id(EntityNamespace::Ordinary, "parc_open");
    let alias = named_id(EntityNamespace::Ordinary, "parc_handle");
    let opaque = named_id(EntityNamespace::Tag, "parc_opaque");
    let packet = named_id(EntityNamespace::Tag, "parc_packet");
    let left = package.retain(&Selection::only([open]).unwrap()).unwrap();
    let right = package.retain(&Selection::only([packet]).unwrap()).unwrap();
    let merged = left.merge(right).unwrap();
    let ids = merged
        .declarations()
        .iter()
        .map(|declaration| declaration.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids, [open, alias, opaque, packet].into_iter().collect());
    assert_eq!(merged.completeness(), &Completeness::Complete);

    let extra_path = "include/second-entry.h";
    let extra_file = FileId::from_logical_path(extra_path).unwrap();
    let extra_bytes = b"\n";
    let mut extra = fixture_input(false);
    extra.files = vec![SourceFile {
        id: extra_file,
        logical_path: extra_path.to_owned(),
        role: SourceFileRole::Entry,
        content: ContentFingerprint::from_content(extra_bytes),
        byte_len: extra_bytes.len() as u64,
        line_starts: vec![0],
    }];
    extra.inputs.entry_files = vec![extra_file];
    extra.declarations.clear();
    extra.macros.clear();
    let merged_entries = package
        .merge(SourcePackage::try_new(extra).unwrap())
        .unwrap();
    let expected_entries = merged_entries
        .inputs()
        .entry_files
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(expected_entries.len(), 2);
    assert!(expected_entries.contains(&extra_file));
    assert_eq!(merged_entries.files().len(), 2);
}

#[test]
fn merge_recomputes_completeness_from_the_diagnostic_union() {
    let mut partial = fixture_input(false);
    let code = DiagnosticCode::new("PARC-P4200").unwrap();
    let message = "composition fixture remains partial".to_owned();
    partial.diagnostics.push(SourceDiagnostic {
        code: code.clone(),
        stage: DiagnosticStage::Contract,
        severity: Severity::Warning,
        completeness_impact: DiagnosticCompletenessImpact::ForcesPartial,
        message: message.clone(),
        range: None,
        related: Vec::new(),
        declaration: None,
        target: partial.target.fingerprint(),
    });
    partial.completeness = Completeness::Partial {
        reasons: vec![CompletenessReason {
            code,
            message,
            range: None,
        }],
    };
    let merged = complete_package()
        .merge(SourcePackage::try_new(partial).unwrap())
        .unwrap();
    assert!(matches!(
        merged.completeness(),
        Completeness::Partial { .. }
    ));
    assert_eq!(merged.diagnostics().len(), 1);
}

#[test]
fn merge_rejects_target_input_and_stable_id_conflicts() {
    let package = complete_package();

    let mut wrong_target = fixture_input(false);
    wrong_target.target = target_with_standard(LanguageStandard::C11);
    assert!(matches!(
        package
            .clone()
            .merge(SourcePackage::try_new(wrong_target).unwrap()),
        Err(ComposeError::IncompatibleTarget)
    ));

    let mut wrong_inputs = fixture_input(false);
    wrong_inputs.inputs.define_events.push(DefineEvent::Define {
        name: "OTHER_MODE".to_owned(),
        value: Some("1".to_owned()),
    });
    assert!(matches!(
        package
            .clone()
            .merge(SourcePackage::try_new(wrong_inputs).unwrap()),
        Err(ComposeError::IncompatibleSourceInputs {
            field: "define_events"
        })
    ));

    let mut conflicting_file = fixture_input(false);
    let file_id = conflicting_file.files[0].id;
    conflicting_file.files[0].content = ContentFingerprint::from_content(b"different content");
    assert!(matches!(
        package
            .clone()
            .merge(SourcePackage::try_new(conflicting_file).unwrap()),
        Err(ComposeError::ConflictingFile { id }) if id == file_id
    ));

    let mut conflicting_declaration = fixture_input(false);
    let open = named_id(EntityNamespace::Ordinary, "parc_open");
    conflicting_declaration
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open)
        .unwrap()
        .linkage = Linkage::None;
    assert!(matches!(
        package.merge(SourcePackage::try_new(conflicting_declaration).unwrap()),
        Err(ComposeError::ConflictingDeclaration { id }) if id == open
    ));
}

#[test]
fn id_algorithm_is_semantic_and_path_mapped() {
    let canonical = FileId::from_logical_path("include/widget.h").unwrap();
    let remapped = FileId::from_logical_path("./include//widget.h").unwrap();
    assert_eq!(canonical, remapped);
    assert_eq!(
        canonical.to_string(),
        "pfile1_69e3e991ae53244537581230fa11e662d9cfac2dffe5a21020f2fd5d8eb8f37f"
    );

    let other_header = FileId::from_logical_path("vendor/widget.h").unwrap();
    let first = DeclarationId::from_entity(
        EntityId::named(
            EntityNamespace::Ordinary,
            EntityScope::File(canonical),
            "same_spelling",
        )
        .unwrap(),
    );
    let second = DeclarationId::from_entity(
        EntityId::named(
            EntityNamespace::Ordinary,
            EntityScope::File(other_header),
            "same_spelling",
        )
        .unwrap(),
    );
    assert_ne!(first, second);

    let tokens = canonical_tokens_bytes(&["int".to_owned(), "f".to_owned()]);
    let occurrence = OccurrenceId::derive(first, canonical, &tokens, 0);
    let whitespace_only = OccurrenceId::derive(first, canonical, &tokens, 0);
    let duplicate = OccurrenceId::derive(first, canonical, &tokens, 1);
    assert_eq!(occurrence, whitespace_only);
    assert_ne!(occurrence, duplicate);
    assert_eq!(
        named_id(EntityNamespace::Ordinary, "parc_open").to_string(),
        "pdecl1_524bcccd395cfaad5d0697f01bc545663e82eaad03be1e515beeb81933f5b37d"
    );
}

#[test]
fn full_width_integer_wire_values_are_canonical_decimal_strings() {
    let values = [
        ExactInteger::signed(i128::MIN),
        ExactInteger::signed(-1),
        ExactInteger::signed(0),
        ExactInteger::signed(i128::MAX),
        ExactInteger::unsigned(0),
        ExactInteger::unsigned(i128::MAX as u128 + 1),
        ExactInteger::unsigned(u128::MAX),
    ];
    for value in values {
        let enumeration = EnumValue::Evaluated { value };
        let encoded = serde_json::to_string(&enumeration).unwrap();
        assert_eq!(
            serde_json::from_str::<EnumValue>(&encoded).unwrap(),
            enumeration
        );

        let macro_value = MacroValue::Integer { value };
        let encoded = serde_json::to_string(&macro_value).unwrap();
        assert_eq!(
            serde_json::from_str::<MacroValue>(&encoded).unwrap(),
            macro_value
        );
    }

    assert_eq!(ExactInteger::signed(-1).as_signed(), Some(-1));
    assert_eq!(ExactInteger::signed(-1).as_unsigned(), None);
    assert_eq!(
        ExactInteger::unsigned(u128::MAX).as_unsigned(),
        Some(u128::MAX)
    );
    assert_eq!(ExactInteger::unsigned(1).as_signed(), None);

    for invalid in [
        "",
        "+1",
        "01",
        "-0",
        " 1",
        "1 ",
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
    ] {
        let encoded =
            format!(r#"{{"state":"evaluated","value":{{"sign":"signed","value":"{invalid}"}}}}"#);
        assert!(
            serde_json::from_str::<EnumValue>(&encoded).is_err(),
            "{invalid:?}"
        );
    }
    for invalid in [
        "-1",
        "+1",
        "01",
        " 1",
        "1 ",
        "340282366920938463463374607431768211456",
    ] {
        let encoded =
            format!(r#"{{"state":"evaluated","value":{{"sign":"unsigned","value":"{invalid}"}}}}"#);
        assert!(
            serde_json::from_str::<EnumValue>(&encoded).is_err(),
            "{invalid:?}"
        );
    }
    assert!(serde_json::from_str::<EnumValue>(
        r#"{"state":"evaluated","value":{"sign":"unsigned","value":7}}"#
    )
    .is_err());
}

#[test]
fn alias_qualified_pointer_validation_is_transitive_and_rejects_non_pointers() {
    let mut transitive = fixture_input(false);
    let alias_id = named_id(EntityNamespace::Ordinary, "parc_handle");
    let base_id = named_id(EntityNamespace::Ordinary, "parc_handle_base");
    let alias_index = transitive
        .declarations
        .iter()
        .position(|declaration| declaration.id == alias_id)
        .unwrap();
    let SourceDeclarationKind::TypeAlias(alias) = &mut transitive.declarations[alias_index].kind
    else {
        unreachable!()
    };
    let pointer_target = alias.target.clone();
    alias.target = supported_type(CTypeKind::AliasRef(base_id));

    let mut base = transitive.declarations[alias_index].clone();
    base.id = base_id;
    base.identity = DeclarationIdentity::Named {
        namespace: EntityNamespace::Ordinary,
        scope: EntityScope::TranslationUnit,
        normalized_name: "parc_handle_base".to_owned(),
    };
    base.name = Some(source_name("parc_handle_base"));
    base.occurrences[0].name_range = None;
    base.occurrences[0].normalized_tokens = vec!["parc_handle_base".to_owned()];
    base.occurrences[0].id = OccurrenceId::derive(
        base_id,
        base.occurrences[0].range.file,
        &canonical_tokens_bytes(&base.occurrences[0].normalized_tokens),
        0,
    );
    base.kind = SourceDeclarationKind::TypeAlias(SourceTypeAlias {
        target: pointer_target,
    });
    transitive.declarations.push(base);
    transitive
        .declarations
        .sort_by_key(|declaration| declaration.id);
    SourcePackage::try_new(transitive).unwrap();

    let mut non_pointer = fixture_input(false);
    let alias = non_pointer
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == alias_id)
        .unwrap();
    let SourceDeclarationKind::TypeAlias(alias) = &mut alias.kind else {
        unreachable!()
    };
    alias.target = supported_type(CTypeKind::Integer(CIntegerType::Int {
        signedness: Signedness::Signed,
    }));
    let error = SourcePackage::try_new(non_pointer).unwrap_err();
    let violations = error.contract_violations().unwrap();
    assert!(violations.iter().any(|violation| {
        violation.code == ContractViolationCode::InvalidType
            && violation.path.ends_with(".qualifiers.is_restrict")
    }));
    assert!(violations.iter().any(|violation| {
        violation.code == ContractViolationCode::InvalidType
            && violation.path.ends_with(".nullability")
    }));
}

#[test]
fn parameter_ids_use_semantic_ordinals_not_optional_names() {
    let parent = named_id(EntityNamespace::Ordinary, "rename_parameters");
    let expected = ChildId::parameter(parent, 0);
    for name in [Some("left"), Some("right"), None] {
        let parameter = SourceParameter {
            id: ChildId::parameter(parent, 0),
            ordinal: 0,
            name: name.map(source_name),
            ty: supported_type(CTypeKind::Integer(CIntegerType::Int {
                signedness: Signedness::Signed,
            })),
            range: range(
                FileId::from_logical_path("include/prototype.h").unwrap(),
                0,
                0,
            ),
            provenance: provenance(),
            attributes: Vec::new(),
            support: SupportStatus::Supported,
        };
        assert_eq!(parameter.id, expected);
    }
    assert!(matches!(
        ChildId::named(parent, ChildRole::Parameter, "left"),
        Err(IdError::ParameterRequiresOrdinal)
    ));
    assert_ne!(expected, ChildId::parameter(parent, 1));
}

#[test]
fn completeness_reasons_and_forcing_diagnostics_are_exactly_correlated() {
    let mut orphan_reason = fixture_input(true);
    orphan_reason.diagnostics.clear();
    assert_invalid_completeness(SourcePackage::try_new(orphan_reason).unwrap_err());

    let mut orphan_diagnostic = fixture_input(true);
    orphan_diagnostic.completeness = Completeness::Partial {
        reasons: Vec::new(),
    };
    assert_invalid_completeness(SourcePackage::try_new(orphan_diagnostic).unwrap_err());

    let mut rejected_as_partial = fixture_input(true);
    rejected_as_partial.diagnostics[0].completeness_impact =
        DiagnosticCompletenessImpact::ForcesRejected;
    assert_invalid_completeness(SourcePackage::try_new(rejected_as_partial).unwrap_err());
}

fn assert_invalid_completeness(error: SourcePackageBuildError) {
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| { violation.code == ContractViolationCode::InvalidCompleteness }));
}

#[test]
fn array_bound_wire_vectors_preserve_all_source_forms_and_wide_bitint_widths() {
    let negative = DiagnosticCode::new("PARC-E4201").unwrap();
    let overflow = DiagnosticCode::new("PARC-E4202").unwrap();
    let vectors = [
        (
            ArrayBound::Fixed { elements: 4 },
            json!({"kind":"fixed","elements":4}),
        ),
        (ArrayBound::Incomplete, json!({"kind":"incomplete"})),
        (ArrayBound::Flexible, json!({"kind":"flexible"})),
        (
            ArrayBound::Variable {
                normalized_expression: "count + 1".to_owned(),
            },
            json!({"kind":"variable","normalized_expression":"count + 1"}),
        ),
        (
            ArrayBound::StaticMinimum {
                minimum: ArrayMinimumBound::Fixed { elements: 8 },
            },
            json!({"kind":"static_minimum","minimum":{"kind":"fixed","elements":8}}),
        ),
        (
            ArrayBound::Invalid {
                spelling: "-1".to_owned(),
                diagnostic: negative,
            },
            json!({"kind":"invalid","spelling":"-1","diagnostic":"PARC-E4201"}),
        ),
        (
            ArrayBound::Invalid {
                spelling: "18446744073709551616".to_owned(),
                diagnostic: overflow,
            },
            json!({"kind":"invalid","spelling":"18446744073709551616","diagnostic":"PARC-E4202"}),
        ),
    ];
    for (bound, expected) in vectors {
        let encoded = serde_json::to_value(&bound).unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(
            serde_json::from_value::<ArrayBound>(encoded).unwrap(),
            bound
        );
    }

    let wide = BitIntWidth::Known {
        bits: u64::from(u16::MAX) + 1,
    };
    assert_eq!(
        serde_json::from_value::<BitIntWidth>(serde_json::to_value(&wide).unwrap()).unwrap(),
        wide
    );
}

#[test]
fn array_parameter_qualifiers_and_bound_context_are_checked() {
    let mut parameter_array = fixture_input(false);
    let open_id = named_id(EntityNamespace::Ordinary, "parc_open");
    let open = parameter_array
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open_id)
        .unwrap();
    let SourceDeclarationKind::Function(open) = &mut open.kind else {
        unreachable!()
    };
    open.parameters[0].ty = supported_type(CTypeKind::Array {
        element: Box::new(supported_type(CTypeKind::Integer(CIntegerType::Int {
            signedness: Signedness::Signed,
        }))),
        bound: ArrayBound::StaticMinimum {
            minimum: ArrayMinimumBound::Variable {
                normalized_expression: "count".to_owned(),
            },
        },
        parameter_qualifiers: TypeQualifiers {
            is_const: true,
            is_volatile: false,
            is_restrict: true,
            is_atomic: false,
        },
    });
    SourcePackage::try_new(parameter_array).unwrap();

    let mut non_parameter = fixture_input(false);
    let packet_id = named_id(EntityNamespace::Tag, "parc_packet");
    let packet = non_parameter
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == packet_id)
        .unwrap();
    let SourceDeclarationKind::Record(packet) = &mut packet.kind else {
        unreachable!()
    };
    packet.fields[0].ty = supported_type(CTypeKind::Array {
        element: Box::new(supported_type(CTypeKind::Integer(CIntegerType::Int {
            signedness: Signedness::Signed,
        }))),
        bound: ArrayBound::Incomplete,
        parameter_qualifiers: TypeQualifiers {
            is_const: true,
            ..TypeQualifiers::NONE
        },
    });
    let error = SourcePackage::try_new(non_parameter).unwrap_err();
    let violations = error.contract_violations().unwrap();
    assert!(violations
        .iter()
        .any(|violation| { violation.path.ends_with(".parameter_qualifiers") }));
    assert!(violations
        .iter()
        .any(|violation| { violation.message.contains("flexible, not incomplete") }));
}

#[test]
fn visibility_states_preserve_source_and_target_provenance() {
    let vectors = [
        (Visibility::Unspecified, "\"unspecified\""),
        (Visibility::ExplicitDefault, "\"explicit_default\""),
        (Visibility::TargetDefault, "\"target_default\""),
        (Visibility::Hidden, "\"hidden\""),
        (Visibility::Protected, "\"protected\""),
        (Visibility::Internal, "\"internal\""),
    ];
    for (visibility, expected) in vectors {
        assert_eq!(serde_json::to_string(&visibility).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<Visibility>(expected).unwrap(),
            visibility
        );
    }
}

#[test]
fn unsupported_declaration_category_is_closed() {
    for category in [
        UnsupportedDeclarationCategory::StaticAssertion,
        UnsupportedDeclarationCategory::InlineAssembly,
        UnsupportedDeclarationCategory::CompilerBuiltin,
        UnsupportedDeclarationCategory::UnsupportedExtension,
        UnsupportedDeclarationCategory::InvalidDeclaration,
    ] {
        let encoded = serde_json::to_string(&category).unwrap();
        assert_eq!(
            serde_json::from_str::<UnsupportedDeclarationCategory>(&encoded).unwrap(),
            category
        );
    }
    assert!(serde_json::from_str::<UnsupportedDeclarationCategory>("\"free_form\"").is_err());
}

#[test]
fn effective_input_invariants_reject_forgery_and_preserve_semantic_sequences() {
    let mut no_entry = fixture_input(false);
    no_entry.inputs.entry_files.clear();
    assert_input_violation(SourcePackage::try_new(no_entry).unwrap_err());

    let mut wrong_role = fixture_input(false);
    wrong_role.files[0].role = SourceFileRole::UserInclude;
    assert_input_violation(SourcePackage::try_new(wrong_role).unwrap_err());

    let mut empty_builtin_version = fixture_input(false);
    empty_builtin_version.inputs.preprocessor = PreprocessorIdentity::Builtin {
        implementation_version: String::new(),
    };
    assert_input_violation(SourcePackage::try_new(empty_builtin_version).unwrap_err());

    let mut bad_external = fixture_input(false);
    bad_external.inputs.preprocessor = PreprocessorIdentity::External {
        executable: "/usr/bin/cc".to_owned(),
        executable_fingerprint: ContentFingerprint::from_content(b"cc"),
        arguments: vec!["-DOK=1".to_owned(), "bad\nargument".to_owned()],
    };
    assert_input_violation(SourcePackage::try_new(bad_external).unwrap_err());

    let mut bad_define = fixture_input(false);
    bad_define.inputs.define_events = vec![DefineEvent::Define {
        name: "1INVALID".to_owned(),
        value: Some("line\nbreak".to_owned()),
    }];
    assert_input_violation(SourcePackage::try_new(bad_define).unwrap_err());

    let mut bad_environment = fixture_input(false);
    bad_environment.inputs.environment = EnvironmentInputs::Captured {
        variables: vec![
            CapturedEnvironment {
                name: "SDKROOT".to_owned(),
                value_fingerprint: ContentFingerprint::from_content(b"sdk"),
            },
            CapturedEnvironment {
                name: "CPATH".to_owned(),
                value_fingerprint: ContentFingerprint::from_content(b"include"),
            },
        ],
    };
    assert_input_violation(SourcePackage::try_new(bad_environment).unwrap_err());

    let mut ordered_repetition = fixture_input(false);
    let entry = ordered_repetition.inputs.entry_files[0];
    let repeated_define = DefineEvent::Define {
        name: "REPEATED".to_owned(),
        value: Some("1".to_owned()),
    };
    ordered_repetition
        .inputs
        .define_events
        .extend([repeated_define.clone(), repeated_define]);
    let repeated_include = IncludeSearchEntry {
        logical_path: "include".to_owned(),
        kind: IncludeSearchKind::User,
        content: None,
    };
    ordered_repetition
        .inputs
        .include_search
        .extend([repeated_include.clone(), repeated_include]);
    ordered_repetition.inputs.forced_includes = vec![entry, entry];
    ordered_repetition.inputs.preprocessor = PreprocessorIdentity::External {
        executable: "toolchains/gcc/bin/gcc".to_owned(),
        executable_fingerprint: ContentFingerprint::from_content(b"gcc"),
        arguments: vec!["-m64".to_owned(), "-m64".to_owned()],
    };
    SourcePackage::try_new(ordered_repetition).unwrap();
}

fn assert_input_violation(error: SourcePackageBuildError) {
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| {
            violation.path.starts_with("inputs.") || violation.path.ends_with(".role")
        }));
}

#[test]
fn diagnostics_and_completeness_reasons_have_canonical_unique_order() {
    let mut canonical = fixture_input(true);
    let mut second = canonical.diagnostics[0].clone();
    second.code = DiagnosticCode::new("PARC-E4101").unwrap();
    second.message = "second forcing source diagnostic".to_owned();
    canonical.diagnostics.push(second.clone());
    canonical.diagnostics.sort();
    let Completeness::Partial { reasons } = &mut canonical.completeness else {
        unreachable!()
    };
    reasons.push(CompletenessReason {
        code: second.code,
        message: second.message,
        range: second.range,
    });
    reasons.sort();
    SourcePackage::try_new(canonical.clone()).unwrap();

    let mut bad_diagnostics = canonical.clone();
    bad_diagnostics.diagnostics.reverse();
    let error = SourcePackage::try_new(bad_diagnostics).unwrap_err();
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| {
            violation.code == ContractViolationCode::NonCanonicalOrder
                && violation.path == "diagnostics"
        }));

    let mut bad_reasons = canonical;
    let Completeness::Partial { reasons } = &mut bad_reasons.completeness else {
        unreachable!()
    };
    reasons.reverse();
    let error = SourcePackage::try_new(bad_reasons).unwrap_err();
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| {
            violation.code == ContractViolationCode::NonCanonicalOrder
                && violation.path == "completeness.reasons"
        }));
}

#[test]
fn checked_ranges_are_contained_nonempty_and_role_consistent() {
    let open_id = named_id(EntityNamespace::Ordinary, "parc_open");

    let mut bad_name_range = fixture_input(false);
    let macro_range = bad_name_range.macros[0].occurrences[0].range;
    let open = bad_name_range
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open_id)
        .unwrap();
    open.occurrences[0].name_range = Some(macro_range);
    assert_range_violation(SourcePackage::try_new(bad_name_range).unwrap_err());

    let mut bad_child_range = fixture_input(false);
    let macro_range = bad_child_range.macros[0].occurrences[0].range;
    let open = bad_child_range
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open_id)
        .unwrap();
    let SourceDeclarationKind::Function(function) = &mut open.kind else {
        unreachable!()
    };
    function.parameters[0].range = macro_range;
    assert_range_violation(SourcePackage::try_new(bad_child_range).unwrap_err());

    let mut empty_occurrence = fixture_input(false);
    let open = empty_occurrence
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open_id)
        .unwrap();
    open.occurrences[0].range.end = open.occurrences[0].range.start;
    open.occurrences[0].spelling.clear();
    open.occurrences[0].normalized_tokens.clear();
    assert_range_violation(SourcePackage::try_new(empty_occurrence).unwrap_err());

    let mut empty_macro = fixture_input(false);
    empty_macro.macros[0].occurrences[0].range.end =
        empty_macro.macros[0].occurrences[0].range.start;
    assert_range_violation(SourcePackage::try_new(empty_macro).unwrap_err());

    let mut wrong_origin = fixture_input(false);
    let open = wrong_origin
        .declarations
        .iter_mut()
        .find(|declaration| declaration.id == open_id)
        .unwrap();
    open.occurrences[0].provenance.origin = SourceOrigin::SystemInclude;
    let error = SourcePackage::try_new(wrong_origin).unwrap_err();
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| {
            violation.path.ends_with(".provenance.origin")
                && violation.code == ContractViolationCode::InvalidFile
        }));
}

fn assert_range_violation(error: SourcePackageBuildError) {
    assert!(error
        .contract_violations()
        .unwrap()
        .iter()
        .any(|violation| { violation.code == ContractViolationCode::InvalidRange }));
}

fn id_golden_vectors() -> Value {
    let canonical_file = FileId::from_logical_path("include/widget.h").unwrap();
    let vendor_file = FileId::from_logical_path("vendor/widget.h").unwrap();
    let same_translation_unit = named_id(EntityNamespace::Ordinary, "same_spelling");
    let same_first_file = DeclarationId::from_entity(
        EntityId::named(
            EntityNamespace::Ordinary,
            EntityScope::File(canonical_file),
            "same_spelling",
        )
        .unwrap(),
    );
    let same_second_file = DeclarationId::from_entity(
        EntityId::named(
            EntityNamespace::Ordinary,
            EntityScope::File(vendor_file),
            "same_spelling",
        )
        .unwrap(),
    );
    let prototype = DeclarationId::from_entity(
        EntityId::named(
            EntityNamespace::Ordinary,
            EntityScope::File(canonical_file),
            "f",
        )
        .unwrap(),
    );
    let prototype_tokens: Vec<_> = ["int", "f", "(", "int", "x", ")", ";"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let token_bytes = canonical_tokens_bytes(&prototype_tokens);
    let declaration_names = ["alpha", "beta", "gamma"];
    let declaration_ids: serde_json::Map<String, Value> = declaration_names
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                json!(named_id(EntityNamespace::Ordinary, name)),
            )
        })
        .collect();
    let base_alias = named_id(EntityNamespace::Ordinary, "base_handle");
    let public_alias = named_id(EntityNamespace::Ordinary, "public_handle");
    let parameter_parent = named_id(EntityNamespace::Ordinary, "rename_parameters");

    json!({
        "schema": "follang.parc.id-golden-vectors",
        "version": 1,
        "id_algorithm_version": ID_ALGORITHM_VERSION,
        "logical_root_remapping": {
            "inputs": ["include/widget.h", "./include//widget.h"],
            "expected_file_id": canonical_file
        },
        "same_spelling_multiple_scopes": {
            "name": "same_spelling",
            "logical_headers": ["include/widget.h", "vendor/widget.h"],
            "translation_unit": same_translation_unit,
            "first_file_scope": same_first_file,
            "second_file_scope": same_second_file
        },
        "whitespace_only_token_normalization": {
            "source_spellings": ["int f(int x);", "int   f ( int x ) ;"],
            "normalized_tokens": prototype_tokens,
            "expected_occurrence_id": OccurrenceId::derive(prototype, canonical_file, &token_bytes, 0)
        },
        "duplicate_prototypes": {
            "declaration": prototype,
            "normalized_tokens": prototype_tokens,
            "ordinals": [0, 1],
            "expected_occurrence_ids": [
                OccurrenceId::derive(prototype, canonical_file, &token_bytes, 0),
                OccurrenceId::derive(prototype, canonical_file, &token_bytes, 1)
            ]
        },
        "declaration_reordering": {
            "permutations": [
                ["alpha", "beta", "gamma"],
                ["gamma", "alpha", "beta"]
            ],
            "expected_declaration_ids": declaration_ids
        },
        "typedef_chain": {
            "chain": ["public_handle", "base_handle"],
            "expected_public_declaration_id": public_alias,
            "expected_base_declaration_id": base_alias
        },
        "parameter_name_independence": {
            "prototype_spellings": [
                "int rename_parameters(int left);",
                "int rename_parameters(int right);",
                "int rename_parameters(int);"
            ],
            "ordinal": 0,
            "expected_parameter_id": ChildId::parameter(parameter_parent, 0)
        }
    })
}

fn preservation_ledger(complete: &SourcePackage, partial: &SourcePackage) -> Value {
    let packet_id = named_id(EntityNamespace::Tag, "parc_packet");
    let mode_id = named_id(EntityNamespace::Tag, "parc_mode");
    let open_id = named_id(EntityNamespace::Ordinary, "parc_open");
    let missing_id = named_id(EntityNamespace::Ordinary, "parc_missing");
    let alias_id = named_id(EntityNamespace::Ordinary, "parc_handle");
    let opaque_id = named_id(EntityNamespace::Tag, "parc_opaque");
    let field_id = match &complete.declaration(packet_id).unwrap().kind {
        SourceDeclarationKind::Record(record) => record.fields[0].id,
        _ => unreachable!(),
    };

    json!({
        "schema": "follang.h1.preservation-ledger",
        "version": 1,
        "source_schema": {
            "id": SOURCE_PACKAGE_SCHEMA_ID,
            "version": SOURCE_PACKAGE_SCHEMA_VERSION
        },
        "target_fingerprint": complete.target_fingerprint(),
        "id_golden_vectors": "id-golden-vectors.json",
        "cases": [
            {
                "name": "complete",
                "artifact": "source-complete.json",
                "source_fingerprint": complete.fingerprint(),
                "expected_completeness": "complete"
            },
            {
                "name": "partial",
                "artifact": "source-partial.json",
                "source_fingerprint": partial.fingerprint(),
                "expected_completeness": "partial"
            }
        ],
        "parc_preservation": [
            {
                "fact": "non_c_calling_convention",
                "declaration": open_id,
                "path": "kind.function.calling_convention",
                "expected": "win64"
            },
            {
                "fact": "alias_pointer_and_pointee_qualifiers",
                "declaration": alias_id,
                "path": "kind.type_alias.target",
                "expected": {"pointer": [], "pointee": ["const", "volatile"]}
            },
            {
                "fact": "parameter_alias_qualifiers_and_nullability",
                "declaration": open_id,
                "child": ChildId::parameter(open_id, 0),
                "path": "kind.function.parameters[0].ty",
                "expected": {"pointer": ["restrict"], "nullability": "nonnull"}
            },
            {
                "fact": "int128_and_extended_float_rejection",
                "artifact": "source-partial.json",
                "declaration": named_id(EntityNamespace::Ordinary, "parc_extended"),
                "expected": "explicit_unsupported",
                "diagnostic": "PARC-E4100"
            },
            {
                "fact": "target_identity",
                "expected": complete.target_fingerprint()
            },
            {
                "fact": "alias_resolution",
                "declaration": alias_id,
                "target_declaration": opaque_id
            },
            {
                "fact": "macro_category_and_value",
                "macro": MacroId::named(complete.files()[0].id, "PARC_ABI_LEVEL").unwrap(),
                "expected": {"category": "abi_affecting", "value": {"sign":"signed", "value":"7"}}
            },
            {
                "fact": "partial_source_diagnostic",
                "artifact": "source-partial.json",
                "code": "PARC-E4100",
                "expected_completeness_impact": "forces_partial"
            },
            {
                "fact": "locations_and_provenance",
                "expected": "all downstream-referenced declarations and children carry exact FileId ranges and SourceProvenance"
            }
        ],
        "linc_evidence_seed": {
            "ownership": "measured ABI and link evidence; not part of PARC SourcePackage",
            "source_fingerprint": complete.fingerprint(),
            "record_layouts": [{
                "declaration": packet_id,
                "size_bits": 32,
                "alignment_bits": 32,
                "fields": [{"child": field_id, "offset_bits": 0, "size_bits": 32}]
            }],
            "enum_representations": [{
                "declaration": mode_id,
                "storage_bits": 32,
                "alignment_bits": 32,
                "signedness": "unsigned"
            }],
            "abi_probe": {
                "machine": "x86_64",
                "object_format": "elf",
                "bitness": 64,
                "endian": "little",
                "abi": "sysv64",
                "linker_flavor": "gnu",
                "crt_flavor": "glibc"
            },
            "providers": [
                {
                    "declaration": open_id,
                    "symbol": "parc_open",
                    "state": "resolved",
                    "provider": "libparc_fixture.a"
                },
                {
                    "declaration": missing_id,
                    "symbol": "parc_missing",
                    "state": "unresolved"
                }
            ],
            "ordered_link_inputs": ["libfirst.a", "librepeat.a", "libmiddle.so", "librepeat.a"]
        }
    })
}

#[test]
fn id_golden_vectors_are_packaged_and_frozen() {
    let frozen: Value = serde_json::from_str(corpus::ID_GOLDEN_VECTORS_JSON).unwrap();
    assert_eq!(frozen, id_golden_vectors());
}

fn assert_source_integrity(package: &SourcePackage) {
    let bytes = corpus::PRESERVATION_HEADER.as_bytes();
    assert_eq!(package.files().len(), 1);
    let file = &package.files()[0];
    assert_eq!(file.content, ContentFingerprint::from_content(bytes));
    assert_eq!(file.byte_len, bytes.len() as u64);
    let mut expected_line_starts = vec![0];
    expected_line_starts.extend(
        bytes
            .iter()
            .enumerate()
            .filter_map(|(index, byte)| (*byte == b'\n').then_some(index as u64 + 1))
            .filter(|start| *start < bytes.len() as u64),
    );
    assert_eq!(file.line_starts, expected_line_starts);

    let slice = |source_range: SourceRange| {
        assert_eq!(source_range.file, file.id);
        &corpus::PRESERVATION_HEADER[source_range.start as usize..source_range.end as usize]
    };
    for declaration in package.declarations() {
        for occurrence in &declaration.occurrences {
            assert_eq!(slice(occurrence.range), occurrence.spelling);
            if let Some(name_range) = occurrence.name_range {
                assert_eq!(
                    slice(name_range),
                    declaration.name.as_ref().unwrap().original
                );
            }
            for attribute in &occurrence.attributes {
                assert_eq!(slice(attribute.range), attribute.spelling);
            }
            if matches!(declaration.kind, SourceDeclarationKind::Function(_)) {
                assert!(!occurrence.is_definition);
            }
        }
        match &declaration.kind {
            SourceDeclarationKind::Function(function) => {
                for parameter in &function.parameters {
                    let expected = match declaration.name.as_ref().unwrap().normalized.as_str() {
                        "parc_open" => "parc_handle restrict handle",
                        "parc_missing" => "struct parc_opaque *handle",
                        "parc_extended" => "__int128 wide",
                        name => panic!("unexpected corpus function parameter owner: {name}"),
                    };
                    assert_eq!(slice(parameter.range), expected);
                }
            }
            SourceDeclarationKind::Record(record) => {
                for field in &record.fields {
                    assert_eq!(slice(field.range), "int value");
                }
            }
            SourceDeclarationKind::Enum(enumeration) => {
                assert!(enumeration.explicit_underlying_type.is_none());
                for variant in &enumeration.variants {
                    assert_eq!(slice(variant.range), "PARC_MODE_FAST = 7");
                }
            }
            SourceDeclarationKind::TypeAlias(_)
            | SourceDeclarationKind::Variable(_)
            | SourceDeclarationKind::Unsupported(_) => {}
        }
    }
    for source_macro in package.macros() {
        for occurrence in &source_macro.occurrences {
            assert_eq!(slice(occurrence.range), "#define PARC_ABI_LEVEL 7");
        }
    }
}

#[test]
fn embedded_preservation_corpus_is_checked_and_canonical() {
    assert_eq!(corpus::preservation_cases().len(), 2);
    for case in corpus::preservation_cases() {
        let package = decode_source_package(case.envelope_json).unwrap();
        assert_eq!(encode_source_package(&package).unwrap(), case.envelope_json);
        assert_source_integrity(&package);
        match case.name {
            "complete" => assert_eq!(package, complete_package()),
            "partial" => assert_eq!(package, partial_package()),
            name => panic!("unexpected preservation case: {name}"),
        }
    }
    let complete = decode_source_package(corpus::COMPLETE_SOURCE_PACKAGE_JSON).unwrap();
    let partial = decode_source_package(corpus::PARTIAL_SOURCE_PACKAGE_JSON).unwrap();
    let ledger: Value = serde_json::from_str(corpus::PRESERVATION_LEDGER_JSON).unwrap();
    assert_eq!(ledger, preservation_ledger(&complete, &partial));
    assert_eq!(ledger["schema"], "follang.h1.preservation-ledger");
    assert_eq!(
        ledger["target_fingerprint"],
        json!(complete.target_fingerprint())
    );
    assert_eq!(
        ledger["cases"][0]["source_fingerprint"],
        json!(complete.fingerprint())
    );
    assert_eq!(
        ledger["cases"][1]["source_fingerprint"],
        json!(partial.fingerprint())
    );
    let providers = ledger["linc_evidence_seed"]["providers"]
        .as_array()
        .unwrap();
    assert!(providers
        .iter()
        .all(|provider| provider["declaration"].is_string()));
    assert_eq!(
        providers[0]["declaration"],
        json!(named_id(EntityNamespace::Ordinary, "parc_open"))
    );
    assert_eq!(
        providers[1]["declaration"],
        json!(named_id(EntityNamespace::Ordinary, "parc_missing"))
    );
}

#[cfg(feature = "system-tests")]
#[test]
fn contract_preservation_gcc_enum_representation() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_NAME: &str = "contract_preservation_gcc_enum_representation";
    if !crate::tests::system_support::begin_system_test(
        TEST_NAME,
        crate::tests::system_support::command_available("gcc"),
        "gcc",
    ) {
        return;
    }
    let probe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("contract-corpus/v2/preservation/probes/enum-representation.c");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_nanos();
    let object = std::env::temp_dir().join(format!("parc-enum-representation-{stamp}.o"));
    let output = Command::new("gcc")
        .args(["-std=gnu17", "-m64", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(&probe)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("execute GCC preservation probe");
    let _ = std::fs::remove_file(&object);
    assert!(
        output.status.success(),
        "GCC preservation probe failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
