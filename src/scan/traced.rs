//! Provenance-carrying built-in preprocessing for the certified H2 scan path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::contract::*;
use crate::preprocess::{
    builtin_headers, parse_directive, Directive, Lexer, MacroDef, MacroTable, Token, TokenKind,
};
use crate::span::Span;

use super::{EnvironmentCapture, ScanConfig, ScanError};

const PREDEFINED_PATH: &str = "__parc_builtin__/target-macros.h";
const COMMAND_LINE_PATH: &str = "__parc_config__/defines.h";

#[derive(Debug, Clone)]
pub(super) struct TraceIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub impact: DiagnosticCompletenessImpact,
    pub message: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone)]
struct TracedToken {
    kind: TokenKind,
    text: String,
    anchor: SourceRange,
    provenance: SourceProvenance,
}

impl TracedToken {
    fn plain(&self) -> Token {
        Token {
            kind: self.kind.clone(),
            text: self.text.clone(),
            offset: usize::try_from(self.anchor.start)
                .expect("scanned inputs are bounded by the producer limits"),
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveMacro {
    definition: MacroDef,
    definition_range: SourceRange,
}

#[derive(Debug, Clone)]
struct MacroDefinitionSnapshot {
    identity_file: FileId,
    name: String,
    form: MacroForm,
    body: String,
    normalized_tokens: Vec<String>,
    range: SourceRange,
    provenance: SourceProvenance,
}

#[derive(Debug, Clone)]
struct LoadedSource {
    id: FileId,
    logical_path: String,
    role: SourceFileRole,
    physical_path: Option<PathBuf>,
    source: String,
}

#[derive(Debug, Clone)]
struct TraceSegment {
    generated_start: usize,
    generated_end: usize,
    anchor: SourceRange,
    provenance: SourceProvenance,
}

#[derive(Debug)]
pub(super) struct TracedPreprocessed {
    pub text: String,
    pub files: BTreeMap<FileId, SourceFile>,
    pub macros: Vec<SourceMacro>,
    pub issues: Vec<TraceIssue>,
    pub used_search_content: BTreeMap<String, ContentFingerprint>,
    segments: Vec<TraceSegment>,
    contents: BTreeMap<FileId, String>,
}

impl TracedPreprocessed {
    pub fn map_span(&self, span: Span) -> Option<(SourceRange, SourceProvenance)> {
        if span.is_none() || span.start > span.end || span.end > self.text.len() {
            return None;
        }
        self.map_generated_range(span.start, span.end)
    }

    pub fn map_generated_range(
        &self,
        start: usize,
        end: usize,
    ) -> Option<(SourceRange, SourceProvenance)> {
        let mut matching = self.segments.iter().filter(|segment| {
            if start == end {
                segment.generated_start <= start && start <= segment.generated_end
            } else {
                segment.generated_start < end && segment.generated_end > start
            }
        });
        let first = matching.next()?;
        let file = first.anchor.file;
        let mut mapped_start = first.anchor.start;
        let mut mapped_end = first.anchor.end;
        let mut provenance = first.provenance.clone();
        for segment in matching {
            if segment.anchor.file != file
                || segment.provenance.origin != provenance.origin
                || segment.provenance.include_chain != provenance.include_chain
            {
                return None;
            }
            mapped_start = mapped_start.min(segment.anchor.start);
            mapped_end = mapped_end.max(segment.anchor.end);
            for expansion in &segment.provenance.macro_expansions {
                if !provenance.macro_expansions.contains(expansion) {
                    provenance.macro_expansions.push(expansion.clone());
                }
            }
        }
        Some((
            SourceRange {
                file,
                start: mapped_start,
                end: mapped_end,
            },
            provenance,
        ))
    }

    pub fn source_text(&self, range: SourceRange) -> Option<&str> {
        let source = self.contents.get(&range.file)?;
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        (start <= end && end <= source.len()).then(|| &source[start..end])
    }
}

#[derive(Debug, Clone, Copy)]
struct ConditionalState {
    any_taken: bool,
    active: bool,
    parent_active: bool,
    else_seen: bool,
}

struct TracedProcessor<'a> {
    config: &'a ScanConfig,
    search_roots: Vec<(PathBuf, IncludeSearchKind, String)>,
    builtins: BTreeMap<String, String>,
    files: BTreeMap<FileId, SourceFile>,
    contents: BTreeMap<FileId, String>,
    loaded: BTreeMap<PathBuf, LoadedSource>,
    active_macros: BTreeMap<String, ActiveMacro>,
    inconsistent_macros: BTreeSet<String>,
    macro_definitions: Vec<MacroDefinitionSnapshot>,
    issues: Vec<TraceIssue>,
    pragma_once: BTreeSet<FileId>,
    used_search_files: BTreeMap<String, Vec<(String, ContentFingerprint)>>,
    total_input_bytes: u64,
    include_count: usize,
    token_count: usize,
    macro_expansions: usize,
    stopped: bool,
}

pub(super) fn preprocess_builtin_traced(
    config: &ScanConfig,
    environment: &EnvironmentCapture,
) -> Result<TracedPreprocessed, ScanError> {
    let mut search_roots = Vec::new();
    for path in &config.include_dirs {
        search_roots.push((
            std::fs::canonicalize(path).map_err(|source| ScanError::Read {
                path: path.display().to_string(),
                source,
            })?,
            IncludeSearchKind::User,
            config.path_mapping.map_path(path)?,
        ));
    }
    for path in &config.system_include_dirs {
        search_roots.push((
            std::fs::canonicalize(path).map_err(|source| ScanError::Read {
                path: path.display().to_string(),
                source,
            })?,
            IncludeSearchKind::System,
            config.path_mapping.map_path(path)?,
        ));
    }
    for (path, kind) in &environment.include_paths {
        search_roots.push((
            std::fs::canonicalize(path).map_err(|source| ScanError::Read {
                path: path.display().to_string(),
                source,
            })?,
            *kind,
            config.path_mapping.map_path(path)?,
        ));
    }

    let mut processor = TracedProcessor {
        config,
        search_roots,
        builtins: builtin_headers().into_iter().collect(),
        files: BTreeMap::new(),
        contents: BTreeMap::new(),
        loaded: BTreeMap::new(),
        active_macros: BTreeMap::new(),
        inconsistent_macros: BTreeSet::new(),
        macro_definitions: Vec::new(),
        issues: Vec::new(),
        pragma_once: BTreeSet::new(),
        used_search_files: BTreeMap::new(),
        total_input_bytes: 0,
        include_count: 0,
        token_count: 0,
        macro_expansions: 0,
        stopped: false,
    };

    processor.install_predefined_macros()?;
    processor.install_command_line_macros()?;

    // Load entries first so a file that is also included has a single stable
    // Entry role throughout provenance validation.
    for path in &config.entry_headers {
        let _ = processor.load_physical(path, SourceFileRole::Entry)?;
    }
    for path in &config.forced_includes {
        let _ = processor.load_physical(path, SourceFileRole::UserInclude)?;
    }

    let mut output = Vec::new();
    for path in &config.forced_includes {
        output.extend(processor.process_physical(
            path,
            SourceFileRole::UserInclude,
            Vec::new(),
            0,
        )?);
    }
    for path in &config.entry_headers {
        output.extend(processor.process_physical(path, SourceFileRole::Entry, Vec::new(), 0)?);
    }

    let mut text = String::new();
    let mut segments = Vec::new();
    for token in output {
        if token.kind == TokenKind::Eof || processor.stopped {
            continue;
        }
        let Some(next_len) = text.len().checked_add(token.text.len()) else {
            processor.limit_issue(
                "PARC-E2207",
                "generated preprocessing output exceeded max_generated_bytes",
                Some(token.anchor),
            );
            break;
        };
        if u64::try_from(next_len).map_or(true, |length| length > config.limits.max_generated_bytes)
        {
            processor.limit_issue(
                "PARC-E2207",
                "generated preprocessing output exceeded max_generated_bytes",
                Some(token.anchor),
            );
            break;
        }
        let start = text.len();
        text.push_str(&token.text);
        segments.push(TraceSegment {
            generated_start: start,
            generated_end: text.len(),
            anchor: token.anchor,
            provenance: token.provenance,
        });
    }

    let generated_id = FileId::from_logical_path(config.path_mapping.generated_path())
        .expect("validated generated logical path");
    let generated = super::source_file(
        generated_id,
        config.path_mapping.generated_path().to_owned(),
        SourceFileRole::Generated,
        text.as_bytes(),
    )?;
    processor.files.insert(generated_id, generated);
    processor.contents.insert(generated_id, text.clone());

    let macros = processor.finish_macros();
    let used_search_content = processor
        .used_search_files
        .into_iter()
        .map(|(root, mut entries)| {
            entries.sort();
            entries.dedup();
            let mut bytes = b"follang.parc.effective-include-search.v1\0".to_vec();
            for (path, content) in entries {
                bytes.extend_from_slice(path.as_bytes());
                bytes.push(0);
                bytes.extend_from_slice(content.as_bytes());
            }
            (root, ContentFingerprint::from_content(&bytes))
        })
        .collect();

    Ok(TracedPreprocessed {
        text,
        files: processor.files,
        macros,
        issues: processor.issues,
        used_search_content,
        segments,
        contents: processor.contents,
    })
}

impl TracedProcessor<'_> {
    fn install_predefined_macros(&mut self) -> Result<(), ScanError> {
        let mut table = MacroTable::new();
        super::define_builtin_target_macros(&mut table, &self.config.target)?;
        let mut definitions = table.all().cloned().collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        self.install_virtual_definitions(PREDEFINED_PATH, SourceFileRole::Builtin, definitions)
    }

    fn install_command_line_macros(&mut self) -> Result<(), ScanError> {
        enum Event {
            Define(MacroDef),
            Undefine(String),
        }

        let mut source = String::new();
        let mut events = Vec::new();
        for event in &self.config.define_events {
            let start = source.len();
            match event {
                DefineEvent::Define { name, value } => {
                    let body = value.as_deref().unwrap_or("1");
                    source.push_str("#define ");
                    source.push_str(name);
                    source.push(' ');
                    source.push_str(body);
                    source.push('\n');
                    events.push((
                        Event::Define(MacroDef {
                            name: name.clone(),
                            params: None,
                            is_variadic: false,
                            body: Lexer::tokenize(body)
                                .into_iter()
                                .filter(|token| {
                                    !matches!(token.kind, TokenKind::Eof | TokenKind::Newline)
                                })
                                .collect(),
                        }),
                        start,
                        source.len(),
                    ));
                }
                DefineEvent::Undefine { name } => {
                    source.push_str("#undef ");
                    source.push_str(name);
                    source.push('\n');
                    events.push((Event::Undefine(name.clone()), start, source.len()));
                }
            }
        }
        if events.is_empty() {
            return Ok(());
        }
        let id = FileId::from_logical_path(COMMAND_LINE_PATH).expect("static logical path");
        self.files.insert(
            id,
            super::source_file(
                id,
                COMMAND_LINE_PATH.to_owned(),
                SourceFileRole::Generated,
                source.as_bytes(),
            )?,
        );
        self.contents.insert(id, source);
        for (event, start, end) in events {
            let range = SourceRange {
                file: id,
                start: u64::try_from(start).map_err(|_| ScanError::SizeOverflow)?,
                end: u64::try_from(end).map_err(|_| ScanError::SizeOverflow)?,
            };
            match event {
                Event::Define(definition) => self.define_macro(
                    definition,
                    range,
                    provenance_for_role(SourceFileRole::Generated, Vec::new()),
                ),
                Event::Undefine(name) => {
                    self.active_macros.remove(&name);
                }
            }
        }
        Ok(())
    }

    fn install_virtual_definitions(
        &mut self,
        logical_path: &str,
        role: SourceFileRole,
        definitions: Vec<MacroDef>,
    ) -> Result<(), ScanError> {
        let mut source = String::new();
        let mut positioned = Vec::new();
        for definition in definitions {
            let start = source.len();
            source.push_str("#define ");
            source.push_str(&definition.name);
            if let Some(parameters) = &definition.params {
                source.push('(');
                source.push_str(&parameters.join(","));
                if definition.is_variadic {
                    if !parameters.is_empty() {
                        source.push(',');
                    }
                    source.push_str("...");
                }
                source.push(')');
            }
            if !definition.body.is_empty() {
                source.push(' ');
                for token in &definition.body {
                    source.push_str(&token.text);
                }
            }
            source.push('\n');
            positioned.push((definition, start, source.len()));
        }
        let id = FileId::from_logical_path(logical_path).expect("static logical path");
        self.files.insert(
            id,
            super::source_file(id, logical_path.to_owned(), role, source.as_bytes())?,
        );
        self.contents.insert(id, source);
        for (definition, start, end) in positioned {
            let range = SourceRange {
                file: id,
                start: u64::try_from(start).map_err(|_| ScanError::SizeOverflow)?,
                end: u64::try_from(end).map_err(|_| ScanError::SizeOverflow)?,
            };
            self.define_macro(definition, range, provenance_for_role(role, Vec::new()));
        }
        Ok(())
    }

    fn load_physical(
        &mut self,
        path: &Path,
        requested_role: SourceFileRole,
    ) -> Result<LoadedSource, ScanError> {
        let canonical = std::fs::canonicalize(path).map_err(|source| ScanError::Read {
            path: path.display().to_string(),
            source,
        })?;
        if let Some(loaded) = self.loaded.get(&canonical) {
            return Ok(loaded.clone());
        }
        let metadata = std::fs::metadata(&canonical).map_err(|source| ScanError::Read {
            path: canonical.display().to_string(),
            source,
        })?;
        if metadata.len() > self.config.limits.max_input_file_bytes {
            return Err(ScanError::ResourceLimit {
                code: "PARC-E2201",
                message: "source file exceeded max_input_file_bytes",
            });
        }
        let remaining_total = self
            .config
            .limits
            .max_total_input_bytes
            .checked_sub(self.total_input_bytes)
            .ok_or(ScanError::ResourceLimit {
                code: "PARC-E2202",
                message: "transitive source bytes exceeded max_total_input_bytes",
            })?;
        if metadata.len() > remaining_total {
            return Err(ScanError::ResourceLimit {
                code: "PARC-E2202",
                message: "transitive source bytes exceeded max_total_input_bytes",
            });
        }
        let cap = self.config.limits.max_input_file_bytes.min(remaining_total);
        let bytes = super::read_bounded_file(&canonical, cap)?.ok_or(ScanError::ResourceLimit {
            code: if self.config.limits.max_input_file_bytes <= remaining_total {
                "PARC-E2201"
            } else {
                "PARC-E2202"
            },
            message: if self.config.limits.max_input_file_bytes <= remaining_total {
                "source file exceeded max_input_file_bytes"
            } else {
                "transitive source bytes exceeded max_total_input_bytes"
            },
        })?;
        let logical_path = self.config.path_mapping.map_path(&canonical)?;
        let id = FileId::from_logical_path(&logical_path).expect("mapped logical path");
        let role = self
            .files
            .get(&id)
            .map_or(requested_role, |existing| existing.role);
        let file = super::source_file(id, logical_path.clone(), role, &bytes)?;
        self.total_input_bytes = self
            .total_input_bytes
            .checked_add(u64::try_from(bytes.len()).map_err(|_| ScanError::SizeOverflow)?)
            .ok_or(ScanError::SizeOverflow)?;
        let source = String::from_utf8(bytes)
            .map_err(|error| ScanError::NonUtf8Output(error.to_string()))?;
        let loaded = LoadedSource {
            id,
            logical_path,
            role,
            physical_path: Some(canonical.clone()),
            source,
        };
        self.files.insert(id, file);
        self.contents.insert(id, loaded.source.clone());
        self.loaded.insert(canonical, loaded.clone());
        Ok(loaded)
    }

    fn load_builtin(&mut self, name: &str, source: &str) -> Result<LoadedSource, ScanError> {
        let logical_path = format!("__parc_builtin__/headers/{name}");
        let id = FileId::from_logical_path(&logical_path).expect("builtin logical path");
        let loaded = LoadedSource {
            id,
            logical_path: logical_path.clone(),
            role: SourceFileRole::Builtin,
            physical_path: None,
            source: source.to_owned(),
        };
        self.files.entry(id).or_insert(super::source_file(
            id,
            logical_path,
            SourceFileRole::Builtin,
            source.as_bytes(),
        )?);
        self.contents.entry(id).or_insert_with(|| source.to_owned());
        Ok(loaded)
    }

    fn process_physical(
        &mut self,
        path: &Path,
        role: SourceFileRole,
        chain: Vec<IncludeSite>,
        depth: usize,
    ) -> Result<Vec<TracedToken>, ScanError> {
        let loaded = self.load_physical(path, role)?;
        self.process_loaded(loaded, chain, depth)
    }

    fn process_loaded(
        &mut self,
        loaded: LoadedSource,
        chain: Vec<IncludeSite>,
        depth: usize,
    ) -> Result<Vec<TracedToken>, ScanError> {
        if self.stopped || self.pragma_once.contains(&loaded.id) {
            return Ok(Vec::new());
        }
        if depth > self.config.limits.max_include_depth {
            self.limit_issue(
                "PARC-E2203",
                "include nesting exceeded max_include_depth",
                chain.last().map(|site| site.directive),
            );
            return Ok(Vec::new());
        }
        let tokens = self.tokenize(&loaded, &chain);
        let mut output = Vec::new();
        let mut pending = Vec::new();
        let mut conditionals = Vec::<ConditionalState>::new();
        let mut index = 0;
        let mut at_line_start = true;

        while index < tokens.len() && !self.stopped {
            let active = conditionals.last().is_none_or(|state| state.active);
            if tokens[index].kind == TokenKind::Hash && at_line_start {
                self.flush_pending(&mut pending, &mut output);
                let directive_start = index;
                index += 1;
                while index < tokens.len() && tokens[index].kind != TokenKind::Newline {
                    index += 1;
                }
                let directive_end = if index < tokens.len() {
                    index += 1;
                    index
                } else {
                    index
                };
                let line = &tokens[directive_start + 1..directive_end];
                let plain = line
                    .iter()
                    .filter(|token| token.kind != TokenKind::Newline)
                    .map(TracedToken::plain)
                    .collect::<Vec<_>>();
                let directive = parse_directive(&plain);
                let range = SourceRange {
                    file: loaded.id,
                    start: tokens[directive_start].anchor.start,
                    end: tokens[directive_end.saturating_sub(1)].anchor.end,
                };
                match directive {
                    Directive::If { tokens } => {
                        let parent_active = active;
                        let branch = parent_active && self.evaluate_condition(&tokens, range);
                        conditionals.push(ConditionalState {
                            any_taken: branch,
                            active: branch,
                            parent_active,
                            else_seen: false,
                        });
                    }
                    Directive::Ifdef { name } => {
                        let parent_active = active;
                        let branch = parent_active && self.active_macros.contains_key(&name);
                        conditionals.push(ConditionalState {
                            any_taken: branch,
                            active: branch,
                            parent_active,
                            else_seen: false,
                        });
                    }
                    Directive::Ifndef { name } => {
                        let parent_active = active;
                        let branch = parent_active && !self.active_macros.contains_key(&name);
                        conditionals.push(ConditionalState {
                            any_taken: branch,
                            active: branch,
                            parent_active,
                            else_seen: false,
                        });
                    }
                    Directive::Elif { tokens } => {
                        let Some(state) = conditionals.last().copied() else {
                            self.unmatched_conditional(range, "#elif without #if");
                            continue;
                        };
                        if state.else_seen {
                            self.malformed_conditional(range, "#elif after #else");
                            if let Some(state) = conditionals.last_mut() {
                                state.active = false;
                            }
                            continue;
                        }
                        let branch = !state.any_taken
                            && state.parent_active
                            && self.evaluate_condition(&tokens, range);
                        if let Some(state) = conditionals.last_mut() {
                            state.active = branch;
                            state.any_taken |= branch;
                        }
                    }
                    Directive::Else => {
                        let Some(state) = conditionals.last_mut() else {
                            self.unmatched_conditional(range, "#else without #if");
                            continue;
                        };
                        if state.else_seen {
                            self.malformed_conditional(range, "duplicate #else");
                            state.active = false;
                            continue;
                        }
                        state.active = state.parent_active && !state.any_taken;
                        state.any_taken = true;
                        state.else_seen = true;
                    }
                    Directive::Endif => {
                        if conditionals.pop().is_none() {
                            self.unmatched_conditional(range, "#endif without #if");
                        }
                    }
                    _ if !active => {}
                    Directive::Define {
                        name,
                        params,
                        is_variadic,
                        body,
                    } => {
                        self.define_macro(
                            MacroDef {
                                name,
                                params,
                                is_variadic,
                                body,
                            },
                            range,
                            provenance_for_role(loaded.role, chain.clone()),
                        );
                    }
                    Directive::Undef { name } => {
                        self.active_macros.remove(&name);
                    }
                    Directive::Include { path, system } => {
                        self.include_count = self.include_count.saturating_add(1);
                        if self.include_count > self.config.limits.max_include_count {
                            self.limit_issue(
                                "PARC-E2204",
                                "include directives exceeded max_include_count",
                                Some(range),
                            );
                            continue;
                        }
                        if unsafe_include_spelling(&path) {
                            self.issues.push(TraceIssue {
                                code: "PARC-E2102",
                                severity: Severity::Error,
                                impact: DiagnosticCompletenessImpact::ForcesRejected,
                                message: format!("unsafe include spelling is rejected: {path}"),
                                range: Some(range),
                            });
                            continue;
                        }
                        match self.resolve_include(&loaded, &path, system)? {
                            Some((included, search_root)) => {
                                if let Some(root) = search_root {
                                    let content = self.files[&included.id].content;
                                    self.used_search_files
                                        .entry(root)
                                        .or_default()
                                        .push((included.logical_path.clone(), content));
                                }
                                let mut include_chain = chain.clone();
                                include_chain.push(IncludeSite {
                                    directive: range,
                                    included: included.id,
                                });
                                output.extend(self.process_loaded(
                                    included,
                                    include_chain,
                                    depth + 1,
                                )?);
                            }
                            None => self.issues.push(TraceIssue {
                                code: "PARC-P2100",
                                severity: Severity::Error,
                                impact: DiagnosticCompletenessImpact::ForcesPartial,
                                message: format!("transitive include was not found: {path}"),
                                range: Some(range),
                            }),
                        }
                    }
                    Directive::Error { message } => self.issues.push(TraceIssue {
                        code: "PARC-E2103",
                        severity: Severity::Error,
                        impact: DiagnosticCompletenessImpact::ForcesRejected,
                        message: format!("active #error directive: {message}"),
                        range: Some(range),
                    }),
                    Directive::Warning { message } => self.issues.push(TraceIssue {
                        code: "PARC-W2100",
                        severity: Severity::Warning,
                        impact: DiagnosticCompletenessImpact::Informational,
                        message: format!("active #warning directive: {message}"),
                        range: Some(range),
                    }),
                    Directive::Pragma { tokens } => {
                        let spelling = tokens
                            .iter()
                            .map(|token| token.text.as_str())
                            .collect::<String>();
                        if spelling.trim() == "once" {
                            self.pragma_once.insert(loaded.id);
                        } else if spelling.trim_start().starts_with("pack") {
                            self.issues.push(TraceIssue {
                                code: "PARC-E2104",
                                severity: Severity::Error,
                                impact: DiagnosticCompletenessImpact::ForcesRejected,
                                message:
                                    "#pragma pack is ABI-relevant and not modeled by H2 lowering"
                                        .to_owned(),
                                range: Some(range),
                            });
                        } else {
                            self.issues.push(TraceIssue {
                                code: "PARC-P2103",
                                severity: Severity::Warning,
                                impact: DiagnosticCompletenessImpact::ForcesPartial,
                                message: format!(
                                    "unknown active pragma is not modeled: {}",
                                    spelling.trim()
                                ),
                                range: Some(range),
                            });
                        }
                    }
                    Directive::Unknown { name, .. } => {
                        let (code, message) = if name == "include_next" {
                            (
                                "PARC-P2102",
                                "#include_next search provenance is unsupported".to_owned(),
                            )
                        } else {
                            (
                                "PARC-P2101",
                                format!("unknown active preprocessing directive: {name}"),
                            )
                        };
                        self.issues.push(TraceIssue {
                            code,
                            severity: Severity::Warning,
                            impact: DiagnosticCompletenessImpact::ForcesPartial,
                            message,
                            range: Some(range),
                        });
                    }
                    Directive::Line { .. } | Directive::LineMarker { .. } => {
                        self.issues.push(TraceIssue {
                            code: "PARC-P2104",
                            severity: Severity::Warning,
                            impact: DiagnosticCompletenessImpact::ForcesPartial,
                            message: "#line and line-marker source remapping is unsupported"
                                .to_owned(),
                            range: Some(range),
                        });
                    }
                    Directive::Null => {}
                }
                output.push(TracedToken {
                    kind: TokenKind::Whitespace,
                    text: "\n".to_owned(),
                    anchor: range,
                    provenance: provenance_for_role(loaded.role, chain.clone()),
                });
                at_line_start = true;
                continue;
            }

            let line_break = matches!(tokens[index].kind, TokenKind::Newline)
                || matches!(
                    tokens[index].kind,
                    TokenKind::LineComment | TokenKind::BlockComment
                ) && tokens[index].text.contains('\n');
            let ignorable_for_line_start = matches!(
                tokens[index].kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
            );
            if active {
                let mut token = tokens[index].clone();
                if token.kind == TokenKind::Hash {
                    self.issues.push(TraceIssue {
                        code: "PARC-E2108",
                        severity: Severity::Error,
                        impact: DiagnosticCompletenessImpact::ForcesRejected,
                        message: "# token appears outside the start of a preprocessing line"
                            .to_owned(),
                        range: Some(token.anchor),
                    });
                }
                match token.kind {
                    TokenKind::LineComment | TokenKind::BlockComment => {
                        token.kind = TokenKind::Whitespace;
                        token.text = " ".to_owned();
                    }
                    TokenKind::Newline => {
                        token.kind = TokenKind::Whitespace;
                        token.text = "\n".to_owned();
                    }
                    TokenKind::Eof => {
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
                pending.push(token);
            }
            if line_break {
                at_line_start = true;
            } else if !ignorable_for_line_start {
                at_line_start = false;
            }
            index += 1;
        }
        self.flush_pending(&mut pending, &mut output);
        if !conditionals.is_empty() {
            self.issues.push(TraceIssue {
                code: "PARC-E2105",
                severity: Severity::Error,
                impact: DiagnosticCompletenessImpact::ForcesRejected,
                message: "source ended with unmatched conditional directives".to_owned(),
                range: Some(SourceRange {
                    file: loaded.id,
                    start: 0,
                    end: self.files[&loaded.id].byte_len,
                }),
            });
        }
        Ok(output)
    }

    fn tokenize(&mut self, loaded: &LoadedSource, chain: &[IncludeSite]) -> Vec<TracedToken> {
        let (spliced, offsets) = splice_with_offsets(&loaded.source);
        let mut lexer = Lexer::new(&spliced);
        let mut result = Vec::new();
        loop {
            let token = lexer.next_token();
            let start = offsets[token.offset.min(offsets.len() - 1)];
            let spliced_end = token
                .offset
                .saturating_add(token.text.len())
                .min(spliced.len());
            let end = offsets[spliced_end.min(offsets.len() - 1)];
            result.push(TracedToken {
                kind: token.kind.clone(),
                text: token.text,
                anchor: SourceRange {
                    file: loaded.id,
                    start: u64::try_from(start)
                        .expect("scanned inputs are bounded by the producer limits"),
                    end: u64::try_from(end)
                        .expect("scanned inputs are bounded by the producer limits"),
                },
                provenance: provenance_for_role(loaded.role, chain.to_vec()),
            });
            if token.kind == TokenKind::Eof {
                break;
            }
        }
        self.token_count = self.token_count.saturating_add(result.len());
        if self.token_count > self.config.limits.max_tokens {
            self.limit_issue(
                "PARC-E2205",
                "preprocessing tokens exceeded max_tokens",
                result.last().map(|token| token.anchor),
            );
        }
        result
    }

    fn flush_pending(&mut self, pending: &mut Vec<TracedToken>, output: &mut Vec<TracedToken>) {
        if pending.is_empty() || self.stopped {
            pending.clear();
            return;
        }
        let expanded = self.expand_tokens(std::mem::take(pending), &mut Vec::new());
        self.token_count = self.token_count.saturating_add(expanded.len());
        if self.token_count > self.config.limits.max_tokens {
            self.limit_issue(
                "PARC-E2205",
                "expanded preprocessing tokens exceeded max_tokens",
                expanded.last().map(|token| token.anchor),
            );
            return;
        }
        output.extend(expanded);
    }

    fn expand_tokens(
        &mut self,
        tokens: Vec<TracedToken>,
        paint: &mut Vec<String>,
    ) -> Vec<TracedToken> {
        let mut result = Vec::new();
        let mut index = 0;
        while index < tokens.len() && !self.stopped {
            let token = &tokens[index];
            let Some(definition) = (token.kind == TokenKind::Ident && !paint.contains(&token.text))
                .then(|| self.active_macros.get(&token.text).cloned())
                .flatten()
            else {
                result.push(token.clone());
                index += 1;
                continue;
            };
            let (arguments, end, invocation) = if definition.definition.params.is_some() {
                let mut next = index + 1;
                while next < tokens.len() && tokens[next].kind == TokenKind::Whitespace {
                    next += 1;
                }
                let Some((arguments, end, invocation)) = collect_arguments(&tokens, index) else {
                    if tokens.get(next).is_some_and(|next| next.text == "(") {
                        self.issues.push(TraceIssue {
                            code: "PARC-E2112",
                            severity: Severity::Error,
                            impact: DiagnosticCompletenessImpact::ForcesRejected,
                            message: format!(
                                "unterminated invocation of function-like macro {}",
                                definition.definition.name
                            ),
                            range: Some(token.anchor),
                        });
                    }
                    result.push(token.clone());
                    index += 1;
                    continue;
                };
                let parameter_count = definition.definition.params.as_ref().map_or(0, Vec::len);
                let argument_count =
                    if parameter_count == 0 && arguments.len() == 1 && arguments[0].is_empty() {
                        0
                    } else {
                        arguments.len()
                    };
                let arity_matches = if definition.definition.is_variadic {
                    argument_count >= parameter_count
                } else {
                    argument_count == parameter_count
                };
                if !arity_matches {
                    self.issues.push(TraceIssue {
                        code: "PARC-E2110",
                        severity: Severity::Error,
                        impact: DiagnosticCompletenessImpact::ForcesRejected,
                        message: format!(
                            "macro {} expected {}{} argument(s) but received {}",
                            definition.definition.name,
                            if definition.definition.is_variadic {
                                "at least "
                            } else {
                                ""
                            },
                            parameter_count,
                            argument_count
                        ),
                        range: Some(invocation),
                    });
                    result.extend(tokens[index..end].iter().cloned());
                    index = end;
                    continue;
                }
                (Some(arguments), end, invocation)
            } else {
                (None, index + 1, token.anchor)
            };
            self.macro_expansions = self.macro_expansions.saturating_add(1);
            if self.macro_expansions > self.config.limits.max_macro_expansions {
                self.limit_issue(
                    "PARC-E2206",
                    "macro invocations exceeded max_macro_expansions",
                    Some(invocation),
                );
                break;
            }
            let expansion = MacroExpansion {
                macro_name: definition.definition.name.clone(),
                invocation,
                definition: Some(definition.definition_range),
            };
            let mut replacement = if let Some(arguments) = arguments {
                substitute_macro(&definition.definition, &arguments, token, invocation)
            } else {
                definition
                    .definition
                    .body
                    .iter()
                    .map(|body| TracedToken {
                        kind: body.kind.clone(),
                        text: body.text.clone(),
                        anchor: invocation,
                        provenance: token.provenance.clone(),
                    })
                    .collect()
            };
            for replacement_token in &mut replacement {
                replacement_token.anchor =
                    combine_range(replacement_token.anchor, invocation).unwrap_or(invocation);
                replacement_token
                    .provenance
                    .macro_expansions
                    .push(expansion.clone());
            }
            if paint.len() >= self.config.limits.max_macro_expansion_depth {
                self.limit_issue(
                    "PARC-E2209",
                    "nested macro expansion exceeded max_macro_expansion_depth",
                    Some(invocation),
                );
                break;
            }
            paint.push(definition.definition.name.clone());
            result.extend(self.expand_tokens(replacement, paint));
            paint.pop();
            index = end;
        }
        result
    }

    fn evaluate_condition(&mut self, tokens: &[Token], range: SourceRange) -> bool {
        let provenance = provenance_for_role(self.files[&range.file].role, Vec::new());
        let mut traced = tokens
            .iter()
            .map(|token| TracedToken {
                kind: token.kind.clone(),
                text: token.text.clone(),
                anchor: range,
                provenance: provenance.clone(),
            })
            .collect::<Vec<_>>();
        if let Err(message) = replace_defined_checked(&mut traced, &self.active_macros) {
            self.issues.push(TraceIssue {
                code: "PARC-E2113",
                severity: Severity::Error,
                impact: DiagnosticCompletenessImpact::ForcesRejected,
                message: format!("invalid conditional expression: {message}"),
                range: Some(range),
            });
            return false;
        }
        let expanded = self.expand_tokens(traced, &mut Vec::new());
        let bits = u32::from(
            self.config
                .target
                .c_data_model()
                .long_long_layout
                .storage_bits,
        );
        match evaluate_checked_condition(&expanded, bits) {
            Ok(value) => value,
            Err(message) => {
                self.issues.push(TraceIssue {
                    code: "PARC-E2113",
                    severity: Severity::Error,
                    impact: DiagnosticCompletenessImpact::ForcesRejected,
                    message: format!(
                        "conditional expression is outside the exact subset: {message}"
                    ),
                    range: Some(range),
                });
                false
            }
        }
    }

    fn define_macro(
        &mut self,
        definition: MacroDef,
        range: SourceRange,
        provenance: SourceProvenance,
    ) {
        if self.macro_definitions.len() >= self.config.limits.max_macro_definitions {
            self.limit_issue(
                "PARC-E2208",
                "macro definitions exceeded max_macro_definitions",
                Some(range),
            );
            return;
        }
        if self
            .active_macros
            .get(&definition.name)
            .is_some_and(|active| !macro_definitions_equal(&active.definition, &definition))
        {
            self.inconsistent_macros.insert(definition.name.clone());
            self.issues.push(TraceIssue {
                code: "PARC-P2110",
                severity: Severity::Warning,
                impact: DiagnosticCompletenessImpact::ForcesPartial,
                message: format!(
                    "macro {} was redefined with a different form or replacement list",
                    definition.name
                ),
                range: Some(range),
            });
        }
        if definition
            .body
            .iter()
            .any(|token| matches!(token.kind, TokenKind::Hash | TokenKind::HashHash))
        {
            self.issues.push(TraceIssue {
                code: "PARC-E2111",
                severity: Severity::Error,
                impact: DiagnosticCompletenessImpact::ForcesRejected,
                message: format!(
                    "macro {} uses unsupported stringification or token pasting",
                    definition.name
                ),
                range: Some(range),
            });
        }
        let normalized_tokens = definition
            .body
            .iter()
            .filter(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Whitespace
                        | TokenKind::Newline
                        | TokenKind::LineComment
                        | TokenKind::BlockComment
                        | TokenKind::Eof
                )
            })
            .map(|token| token.text.clone())
            .collect::<Vec<_>>();
        let body = definition
            .body
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>();
        self.macro_definitions.push(MacroDefinitionSnapshot {
            identity_file: range.file,
            name: definition.name.clone(),
            form: if definition.params.is_some() {
                MacroForm::FunctionLike
            } else {
                MacroForm::ObjectLike
            },
            body,
            normalized_tokens,
            range,
            provenance,
        });
        self.active_macros.insert(
            definition.name.clone(),
            ActiveMacro {
                definition,
                definition_range: range,
            },
        );
    }

    fn finish_macros(&mut self) -> Vec<SourceMacro> {
        let active_ranges = self
            .active_macros
            .iter()
            .map(|(name, active)| (name.clone(), active.definition_range))
            .collect::<BTreeMap<_, _>>();
        let mut grouped = BTreeMap::<(FileId, String), Vec<MacroDefinitionSnapshot>>::new();
        for definition in std::mem::take(&mut self.macro_definitions) {
            grouped
                .entry((definition.identity_file, definition.name.clone()))
                .or_default()
                .push(definition);
        }
        let mut macros = Vec::new();
        for ((file, name), definitions) in grouped {
            let Some(active_range) = active_ranges.get(&name) else {
                continue;
            };
            let Some(chosen) = definitions
                .iter()
                .find(|definition| definition.range == *active_range)
            else {
                continue;
            };
            let id = MacroId::named(file, &name).expect("preprocessor macro identifier");
            let support = if self.inconsistent_macros.contains(&name) {
                let reason = "macro was redefined with a different form or replacement list";
                SupportStatus::Partial {
                    code: diagnostic_code("PARC-P2110"),
                    reason: reason.to_owned(),
                }
            } else {
                SupportStatus::Supported
            };
            let mut occurrences = definitions
                .iter()
                .enumerate()
                .map(|(ordinal, definition)| {
                    let duplicate_ordinal = u64::try_from(ordinal)
                        .expect("macro definitions are bounded by max_macro_definitions");
                    MacroOccurrence {
                        id: OccurrenceId::derive_macro(
                            id,
                            definition.range.file,
                            &canonical_tokens_bytes(&definition.normalized_tokens),
                            duplicate_ordinal,
                        ),
                        range: definition.range,
                        normalized_tokens: definition.normalized_tokens.clone(),
                        duplicate_ordinal,
                        provenance: definition.provenance.clone(),
                    }
                })
                .collect::<Vec<_>>();
            occurrences.sort_by_key(|occurrence| occurrence.id);
            let value = macro_value(&chosen.normalized_tokens);
            macros.push(SourceMacro {
                id,
                identity_file: file,
                name,
                form: chosen.form,
                category: if chosen.form == MacroForm::ObjectLike
                    && chosen.normalized_tokens.is_empty()
                {
                    MacroCategory::ConfigurationFlag
                } else if value.is_some() {
                    MacroCategory::BindableConstant
                } else {
                    MacroCategory::AbiAffecting
                },
                body: chosen.body.clone(),
                normalized_tokens: chosen.normalized_tokens.clone(),
                value,
                occurrences,
                support,
            });
        }
        macros.sort_by_key(|macro_item| macro_item.id);
        macros
    }

    fn resolve_include(
        &mut self,
        including: &LoadedSource,
        name: &str,
        system: bool,
    ) -> Result<Option<(LoadedSource, Option<String>)>, ScanError> {
        let mut candidates = Vec::<(PathBuf, SourceFileRole, Option<String>)>::new();
        if !system {
            if let Some(parent) = including.physical_path.as_deref().and_then(Path::parent) {
                candidates.push((parent.join(name), SourceFileRole::UserInclude, None));
            }
        }
        for (root, kind, logical_root) in &self.search_roots {
            let role = if *kind == IncludeSearchKind::System {
                SourceFileRole::SystemInclude
            } else {
                SourceFileRole::UserInclude
            };
            candidates.push((root.join(name), role, Some(logical_root.clone())));
        }
        for (candidate, role, root) in candidates {
            if !candidate.is_file() {
                continue;
            }
            let canonical =
                std::fs::canonicalize(&candidate).map_err(|source| ScanError::Read {
                    path: candidate.display().to_string(),
                    source,
                })?;
            // Mapping the canonical target is the symlink policy: a link that
            // escapes every explicit root is rejected rather than recorded by
            // its lexical spelling.
            self.config.path_mapping.map_path(&canonical)?;
            let loaded = self.load_physical(&canonical, role)?;
            return Ok(Some((loaded, root)));
        }
        if let Some(source) = self.builtins.get(name).cloned() {
            return Ok(Some((self.load_builtin(name, &source)?, None)));
        }
        Ok(None)
    }

    fn unmatched_conditional(&mut self, range: SourceRange, message: &str) {
        self.issues.push(TraceIssue {
            code: "PARC-E2106",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: message.to_owned(),
            range: Some(range),
        });
    }

    fn malformed_conditional(&mut self, range: SourceRange, message: &str) {
        self.issues.push(TraceIssue {
            code: "PARC-E2107",
            severity: Severity::Error,
            impact: DiagnosticCompletenessImpact::ForcesRejected,
            message: message.to_owned(),
            range: Some(range),
        });
    }

    fn limit_issue(&mut self, code: &'static str, message: &str, range: Option<SourceRange>) {
        if !self.stopped {
            self.issues.push(TraceIssue {
                code,
                severity: Severity::Error,
                impact: DiagnosticCompletenessImpact::ForcesRejected,
                message: message.to_owned(),
                range,
            });
        }
        self.stopped = true;
    }
}

fn provenance_for_role(role: SourceFileRole, include_chain: Vec<IncludeSite>) -> SourceProvenance {
    SourceProvenance {
        origin: match role {
            SourceFileRole::Entry => SourceOrigin::Entry,
            SourceFileRole::UserInclude => SourceOrigin::UserInclude,
            SourceFileRole::SystemInclude => SourceOrigin::SystemInclude,
            SourceFileRole::Builtin => SourceOrigin::Builtin,
            SourceFileRole::Generated => SourceOrigin::Generated,
        },
        include_chain,
        macro_expansions: Vec::new(),
    }
}

fn unsafe_include_spelling(path: &str) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn splice_with_offsets(source: &str) -> (String, Vec<usize>) {
    let bytes = source.as_bytes();
    let mut spliced = Vec::with_capacity(bytes.len());
    let mut offsets = Vec::with_capacity(bytes.len() + 1);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && bytes.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        if bytes[index] == b'\\'
            && bytes.get(index + 1) == Some(&b'\r')
            && bytes.get(index + 2) == Some(&b'\n')
        {
            index += 3;
            continue;
        }
        offsets.push(index);
        spliced.push(bytes[index]);
        index += 1;
    }
    offsets.push(bytes.len());
    (
        String::from_utf8(spliced).expect("source was validated UTF-8"),
        offsets,
    )
}

fn collect_arguments(
    tokens: &[TracedToken],
    macro_index: usize,
) -> Option<(Vec<Vec<TracedToken>>, usize, SourceRange)> {
    let mut index = macro_index + 1;
    while index < tokens.len() && tokens[index].kind == TokenKind::Whitespace {
        index += 1;
    }
    if tokens.get(index)?.text != "(" {
        return None;
    }
    let mut arguments = vec![Vec::new()];
    let mut depth = 0usize;
    index += 1;
    while index < tokens.len() {
        match tokens[index].text.as_str() {
            "(" => {
                depth += 1;
                arguments.last_mut()?.push(tokens[index].clone());
            }
            ")" if depth == 0 => {
                let invocation = combine_range(tokens[macro_index].anchor, tokens[index].anchor)?;
                for argument in &mut arguments {
                    while argument
                        .first()
                        .is_some_and(|token| token.kind == TokenKind::Whitespace)
                    {
                        argument.remove(0);
                    }
                    while argument
                        .last()
                        .is_some_and(|token| token.kind == TokenKind::Whitespace)
                    {
                        argument.pop();
                    }
                }
                return Some((arguments, index + 1, invocation));
            }
            ")" => {
                depth -= 1;
                arguments.last_mut()?.push(tokens[index].clone());
            }
            "," if depth == 0 => arguments.push(Vec::new()),
            _ => arguments.last_mut()?.push(tokens[index].clone()),
        }
        index += 1;
    }
    None
}

fn substitute_macro(
    definition: &MacroDef,
    arguments: &[Vec<TracedToken>],
    invocation_token: &TracedToken,
    invocation: SourceRange,
) -> Vec<TracedToken> {
    let parameters = definition.params.as_deref().unwrap_or_default();
    let mut result = Vec::<TracedToken>::new();
    let mut index = 0;
    while index < definition.body.len() {
        let token = &definition.body[index];
        if token.kind == TokenKind::Hash {
            let mut next = index + 1;
            while next < definition.body.len()
                && definition.body[next].kind == TokenKind::Whitespace
            {
                next += 1;
            }
            if let Some(parameter) = definition.body.get(next) {
                if let Some(argument_index) =
                    parameters.iter().position(|name| name == &parameter.text)
                {
                    let text =
                        stringify(&arguments.get(argument_index).cloned().unwrap_or_default());
                    result.push(TracedToken {
                        kind: TokenKind::StringLiteral,
                        text,
                        anchor: invocation,
                        provenance: invocation_token.provenance.clone(),
                    });
                    index = next + 1;
                    continue;
                }
            }
        }
        if token.kind == TokenKind::HashHash {
            while result
                .last()
                .is_some_and(|token| token.kind == TokenKind::Whitespace)
            {
                result.pop();
            }
            index += 1;
            while index < definition.body.len()
                && definition.body[index].kind == TokenKind::Whitespace
            {
                index += 1;
            }
            if let Some(right) = definition.body.get(index) {
                let right_text = parameters
                    .iter()
                    .position(|name| name == &right.text)
                    .and_then(|argument_index| arguments.get(argument_index))
                    .map(|argument| argument.iter().map(|token| token.text.as_str()).collect())
                    .unwrap_or_else(|| right.text.clone());
                if let Some(left) = result.last_mut() {
                    left.text.push_str(&right_text);
                    left.anchor = invocation;
                }
                index += 1;
            }
            continue;
        }
        if token.kind == TokenKind::Ident {
            if token.text == "__VA_ARGS__" && definition.is_variadic {
                for (argument_index, argument) in
                    arguments.iter().skip(parameters.len()).enumerate()
                {
                    if argument_index > 0 {
                        result.push(TracedToken {
                            kind: TokenKind::Punct,
                            text: ",".to_owned(),
                            anchor: invocation,
                            provenance: invocation_token.provenance.clone(),
                        });
                    }
                    result.extend(argument.iter().cloned());
                }
                index += 1;
                continue;
            }
            if let Some(argument_index) = parameters.iter().position(|name| name == &token.text) {
                result.extend(arguments.get(argument_index).into_iter().flatten().cloned());
                index += 1;
                continue;
            }
        }
        result.push(TracedToken {
            kind: token.kind.clone(),
            text: token.text.clone(),
            anchor: invocation,
            provenance: invocation_token.provenance.clone(),
        });
        index += 1;
    }
    result
}

fn stringify(tokens: &[TracedToken]) -> String {
    let mut value = String::from("\"");
    for character in tokens.iter().flat_map(|token| token.text.chars()) {
        if matches!(character, '\\' | '"') {
            value.push('\\');
        }
        value.push(character);
    }
    value.push('"');
    value
}

fn combine_range(left: SourceRange, right: SourceRange) -> Option<SourceRange> {
    (left.file == right.file).then_some(SourceRange {
        file: left.file,
        start: left.start.min(right.start),
        end: left.end.max(right.end),
    })
}

fn macro_definitions_equal(left: &MacroDef, right: &MacroDef) -> bool {
    left.params == right.params
        && left.is_variadic == right.is_variadic
        && normalized_macro_body(&left.body) == normalized_macro_body(&right.body)
}

fn normalized_macro_body(tokens: &[Token]) -> Vec<(TokenKind, String)> {
    tokens
        .iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Whitespace
                    | TokenKind::Newline
                    | TokenKind::LineComment
                    | TokenKind::BlockComment
                    | TokenKind::Eof
            )
        })
        .map(|token| (token.kind.clone(), token.text.clone()))
        .collect()
}

fn replace_defined_checked(
    tokens: &mut Vec<TracedToken>,
    macros: &BTreeMap<String, ActiveMacro>,
) -> Result<(), &'static str> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].kind == TokenKind::Ident && tokens[index].text == "defined" {
            let mut anchor = tokens[index].anchor;
            let provenance = tokens[index].provenance.clone();
            index += 1;
            while index < tokens.len() && tokens[index].kind == TokenKind::Whitespace {
                index += 1;
            }
            let parenthesized = tokens.get(index).is_some_and(|token| token.text == "(");
            if parenthesized {
                anchor = combine_range(anchor, tokens[index].anchor).unwrap_or(anchor);
                index += 1;
                while index < tokens.len() && tokens[index].kind == TokenKind::Whitespace {
                    index += 1;
                }
            }
            let Some(name_token) = tokens.get(index) else {
                return Err("defined has no identifier operand");
            };
            if name_token.kind != TokenKind::Ident {
                return Err("defined operand is not an identifier");
            }
            let name = name_token.text.clone();
            anchor = combine_range(anchor, name_token.anchor).unwrap_or(anchor);
            index += 1;
            if parenthesized {
                while index < tokens.len() && tokens[index].kind == TokenKind::Whitespace {
                    index += 1;
                }
                let Some(close) = tokens.get(index) else {
                    return Err("parenthesized defined operand is missing ')'");
                };
                if close.text != ")" {
                    return Err("parenthesized defined operand has trailing tokens");
                }
                anchor = combine_range(anchor, close.anchor).unwrap_or(anchor);
                index += 1;
            }
            output.push(TracedToken {
                kind: TokenKind::Number,
                text: if macros.contains_key(&name) { "1" } else { "0" }.to_owned(),
                anchor,
                provenance,
            });
        } else {
            output.push(tokens[index].clone());
            index += 1;
        }
    }
    *tokens = output;
    Ok(())
}

#[derive(Debug)]
enum ConditionExpr {
    Value(i128),
    Unary(&'static str, Box<ConditionExpr>),
    Binary(&'static str, Box<ConditionExpr>, Box<ConditionExpr>),
    Ternary(Box<ConditionExpr>, Box<ConditionExpr>, Box<ConditionExpr>),
}

struct ConditionParser<'a> {
    tokens: Vec<&'a TracedToken>,
    position: usize,
    signed_max: i128,
}

fn evaluate_checked_condition(
    tokens: &[TracedToken],
    signed_width: u32,
) -> Result<bool, &'static str> {
    if !(2..=128).contains(&signed_width) {
        return Err("target intmax width is unsupported");
    }
    let signed_max = if signed_width == 128 {
        i128::MAX
    } else {
        (1_i128 << (signed_width - 1)) - 1
    };
    let mut parser = ConditionParser {
        tokens: tokens
            .iter()
            .filter(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Whitespace
                        | TokenKind::Newline
                        | TokenKind::LineComment
                        | TokenKind::BlockComment
                        | TokenKind::Eof
                )
            })
            .collect(),
        position: 0,
        signed_max,
    };
    if parser.tokens.is_empty() {
        return Err("empty expression");
    }
    let expression = parser.ternary()?;
    if parser.peek().is_some() {
        return Err("trailing tokens");
    }
    Ok(evaluate_condition_expr(&expression, signed_width, signed_max)? != 0)
}

impl ConditionParser<'_> {
    fn peek(&self) -> Option<&TracedToken> {
        self.tokens.get(self.position).copied()
    }

    fn consume(&mut self, text: &str) -> bool {
        if self.peek().is_some_and(|token| token.text == text) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn ternary(&mut self) -> Result<ConditionExpr, &'static str> {
        let condition = self.logical_or()?;
        if !self.consume("?") {
            return Ok(condition);
        }
        let then_value = self.ternary()?;
        if !self.consume(":") {
            return Err("ternary expression is missing ':'");
        }
        let else_value = self.ternary()?;
        Ok(ConditionExpr::Ternary(
            Box::new(condition),
            Box::new(then_value),
            Box::new(else_value),
        ))
    }

    fn logical_or(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::logical_and, &["||"])
    }

    fn logical_and(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::bitwise_or, &["&&"])
    }

    fn bitwise_or(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::bitwise_xor, &["|"])
    }

    fn bitwise_xor(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::bitwise_and, &["^"])
    }

    fn bitwise_and(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::equality, &["&"])
    }

    fn equality(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::relational, &["==", "!="])
    }

    fn relational(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::shift, &["<", ">", "<=", ">="])
    }

    fn shift(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::additive, &["<<", ">>"])
    }

    fn additive(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::multiplicative, &["+", "-"])
    }

    fn multiplicative(&mut self) -> Result<ConditionExpr, &'static str> {
        self.binary_chain(Self::unary, &["*", "/", "%"])
    }

    fn binary_chain(
        &mut self,
        next: fn(&mut Self) -> Result<ConditionExpr, &'static str>,
        operators: &[&'static str],
    ) -> Result<ConditionExpr, &'static str> {
        let mut left = next(self)?;
        while let Some(operator) = self
            .peek()
            .and_then(|token| operators.iter().copied().find(|value| *value == token.text))
        {
            self.position += 1;
            let right = next(self)?;
            left = ConditionExpr::Binary(operator, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<ConditionExpr, &'static str> {
        for operator in ["!", "-", "+", "~"] {
            if self.consume(operator) {
                return Ok(ConditionExpr::Unary(operator, Box::new(self.unary()?)));
            }
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<ConditionExpr, &'static str> {
        if self.consume("(") {
            let value = self.ternary()?;
            if !self.consume(")") {
                return Err("parenthesized expression is missing ')'");
            }
            return Ok(value);
        }
        let Some(token) = self.peek() else {
            return Err("missing operand");
        };
        let kind = token.kind.clone();
        let text = token.text.clone();
        self.position += 1;
        match kind {
            TokenKind::Number => {
                parse_condition_integer(&text, self.signed_max).map(ConditionExpr::Value)
            }
            TokenKind::Ident => Ok(ConditionExpr::Value(0)),
            TokenKind::CharLiteral => Err("character constants require an execution character set"),
            _ => Err("unsupported preprocessing token"),
        }
    }
}

fn parse_condition_integer(text: &str, signed_max: i128) -> Result<i128, &'static str> {
    let suffix_start = text
        .rfind(|character: char| !matches!(character, 'u' | 'U' | 'l' | 'L'))
        .map_or(0, |index| {
            index + text[index..].chars().next().map_or(0, char::len_utf8)
        });
    let (number, suffix) = text.split_at(suffix_start);
    if suffix.bytes().any(|byte| matches!(byte, b'u' | b'U')) {
        return Err("unsigned integer semantics are outside the certified subset");
    }
    if suffix.len() > 2 || !suffix.bytes().all(|byte| matches!(byte, b'l' | b'L')) {
        return Err("invalid integer suffix");
    }
    let (radix, digits) = if let Some(value) = number
        .strip_prefix("0x")
        .or_else(|| number.strip_prefix("0X"))
    {
        (16, value)
    } else if let Some(value) = number
        .strip_prefix("0b")
        .or_else(|| number.strip_prefix("0B"))
    {
        (2, value)
    } else if number.starts_with('0') && number.len() > 1 {
        (8, &number[1..])
    } else {
        (10, number)
    };
    if digits.is_empty() {
        return Err("invalid integer literal");
    }
    let value = u128::from_str_radix(digits, radix).map_err(|_| "invalid integer literal")?;
    if value > signed_max as u128 {
        return Err("integer literal exceeds target intmax range");
    }
    Ok(value as i128)
}

fn evaluate_condition_expr(
    expression: &ConditionExpr,
    signed_width: u32,
    signed_max: i128,
) -> Result<i128, &'static str> {
    let signed_min = -signed_max - 1;
    let checked = |value: i128| {
        if (signed_min..=signed_max).contains(&value) {
            Ok(value)
        } else {
            Err("signed arithmetic overflow")
        }
    };
    match expression {
        ConditionExpr::Value(value) => Ok(*value),
        ConditionExpr::Unary(operator, value) => {
            let value = evaluate_condition_expr(value, signed_width, signed_max)?;
            match *operator {
                "!" => Ok(i128::from(value == 0)),
                "+" => Ok(value),
                "-" => checked(value.checked_neg().ok_or("signed arithmetic overflow")?),
                "~" => Err("bitwise complement requires target representation semantics"),
                _ => Err("unsupported unary operator"),
            }
        }
        ConditionExpr::Binary("&&", left, right) => {
            let left = evaluate_condition_expr(left, signed_width, signed_max)?;
            if left == 0 {
                Ok(0)
            } else {
                Ok(i128::from(
                    evaluate_condition_expr(right, signed_width, signed_max)? != 0,
                ))
            }
        }
        ConditionExpr::Binary("||", left, right) => {
            let left = evaluate_condition_expr(left, signed_width, signed_max)?;
            if left != 0 {
                Ok(1)
            } else {
                Ok(i128::from(
                    evaluate_condition_expr(right, signed_width, signed_max)? != 0,
                ))
            }
        }
        ConditionExpr::Binary(operator, left, right) => {
            let left = evaluate_condition_expr(left, signed_width, signed_max)?;
            let right = evaluate_condition_expr(right, signed_width, signed_max)?;
            match *operator {
                "+" => checked(
                    left.checked_add(right)
                        .ok_or("signed arithmetic overflow")?,
                ),
                "-" => checked(
                    left.checked_sub(right)
                        .ok_or("signed arithmetic overflow")?,
                ),
                "*" => checked(
                    left.checked_mul(right)
                        .ok_or("signed arithmetic overflow")?,
                ),
                "/" => {
                    if right == 0 {
                        Err("division by zero")
                    } else {
                        checked(
                            left.checked_div(right)
                                .ok_or("signed arithmetic overflow")?,
                        )
                    }
                }
                "%" => {
                    if right == 0 {
                        Err("remainder by zero")
                    } else {
                        checked(
                            left.checked_rem(right)
                                .ok_or("signed arithmetic overflow")?,
                        )
                    }
                }
                "<<" => {
                    let shift = u32::try_from(right).map_err(|_| "negative shift count")?;
                    if shift >= signed_width || left < 0 {
                        return Err("invalid left shift");
                    }
                    checked(left.checked_shl(shift).ok_or("invalid left shift")?)
                }
                ">>" => {
                    let shift = u32::try_from(right).map_err(|_| "negative shift count")?;
                    if shift >= signed_width || left < 0 {
                        return Err("implementation-defined right shift");
                    }
                    Ok(left >> shift)
                }
                "&" | "|" | "^" if left < 0 || right < 0 => {
                    Err("bitwise operation on a negative value requires representation semantics")
                }
                "&" => Ok(left & right),
                "|" => Ok(left | right),
                "^" => Ok(left ^ right),
                "==" => Ok(i128::from(left == right)),
                "!=" => Ok(i128::from(left != right)),
                "<" => Ok(i128::from(left < right)),
                ">" => Ok(i128::from(left > right)),
                "<=" => Ok(i128::from(left <= right)),
                ">=" => Ok(i128::from(left >= right)),
                _ => Err("unsupported binary operator"),
            }
        }
        ConditionExpr::Ternary(condition, then_value, else_value) => {
            if evaluate_condition_expr(condition, signed_width, signed_max)? != 0 {
                evaluate_condition_expr(then_value, signed_width, signed_max)
            } else {
                evaluate_condition_expr(else_value, signed_width, signed_max)
            }
        }
    }
}

fn macro_value(tokens: &[String]) -> Option<MacroValue> {
    if tokens.len() != 1 {
        return (!tokens.is_empty()).then(|| MacroValue::Tokens {
            tokens: tokens.to_vec(),
        });
    }
    let token = &tokens[0];
    if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
        return Some(MacroValue::String {
            value: token[1..token.len() - 1].to_owned(),
        });
    }
    if let Some(value) = parse_integer(token) {
        return Some(MacroValue::Integer { value });
    }
    Some(MacroValue::Tokens {
        tokens: tokens.to_vec(),
    })
}

fn parse_integer(token: &str) -> Option<ExactInteger> {
    let suffix_start = token.find(['u', 'U', 'l', 'L']).unwrap_or(token.len());
    let (number, suffix) = token.split_at(suffix_start);
    let (radix, digits) = if let Some(value) = number
        .strip_prefix("0x")
        .or_else(|| number.strip_prefix("0X"))
    {
        (16, value)
    } else if let Some(value) = number
        .strip_prefix("0b")
        .or_else(|| number.strip_prefix("0B"))
    {
        (2, value)
    } else if number.starts_with('0') && number.len() > 1 {
        (8, &number[1..])
    } else {
        (10, number)
    };
    let magnitude = u128::from_str_radix(digits, radix).ok()?;
    if suffix.bytes().any(|byte| matches!(byte, b'u' | b'U')) {
        Some(ExactInteger::unsigned(magnitude))
    } else {
        i128::try_from(magnitude).ok().map(ExactInteger::signed)
    }
}

fn diagnostic_code(value: &str) -> DiagnosticCode {
    DiagnosticCode::new(value).expect("static diagnostic code")
}
