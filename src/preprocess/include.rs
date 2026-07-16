use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::lexer::Lexer;
use super::macros::MacroTable;
use super::processor::{tokens_to_text, Processor};
use super::token::{Token, TokenKind};

/// Include resolver: handles `#include` directives by searching paths
/// and recursively preprocessing included files.
pub struct IncludeResolver {
    /// Search paths for `"local"` includes (searched first).
    local_paths: Vec<PathBuf>,
    /// Search paths for `<system>` includes.
    system_paths: Vec<PathBuf>,
    /// Include guards: tracks headers that have already been fully included.
    guards: HashSet<String>,
    /// Files that have used `#pragma once`.
    pragma_once_files: HashSet<PathBuf>,
    /// File content cache: avoids re-reading files from disk.
    file_cache: HashMap<PathBuf, String>,
    /// Built-in headers (e.g. stdint.h, stddef.h) keyed by header name.
    builtin_headers: HashMap<String, String>,
    /// Maximum include depth to prevent infinite recursion.
    max_depth: usize,
    /// Current include depth.
    depth: usize,
    /// Current source directory (for relative includes).
    current_dir: Option<PathBuf>,
}

/// Result of a full preprocessing run with includes resolved.
pub struct PreprocessResult {
    /// The final output text after all includes and macros are resolved.
    pub text: String,
    /// The final output tokens.
    pub tokens: Vec<Token>,
    /// All errors encountered.
    pub errors: Vec<String>,
    /// All warnings encountered.
    pub warnings: Vec<String>,
}

impl Default for IncludeResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl IncludeResolver {
    pub fn new() -> Self {
        IncludeResolver {
            local_paths: Vec::new(),
            system_paths: Vec::new(),
            guards: HashSet::new(),
            pragma_once_files: HashSet::new(),
            file_cache: HashMap::new(),
            builtin_headers: HashMap::new(),
            max_depth: 200,
            depth: 0,
            current_dir: None,
        }
    }

    /// Register built-in headers (e.g. stdint.h, stddef.h).
    /// These are used as fallback when the header isn't found on disk.
    pub fn register_builtin_headers(&mut self, headers: HashMap<String, String>) {
        self.builtin_headers.extend(headers);
    }

    /// Add a local include search path (for `"..."` includes).
    pub fn add_local_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.local_paths.push(path.into());
    }

    /// Add a system include search path (for `<...>` includes).
    pub fn add_system_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.system_paths.push(path.into());
    }

    /// Resolve an include path to a filesystem path.
    pub fn resolve(&self, path: &str, system: bool) -> Option<PathBuf> {
        if !system {
            // Local includes: search relative to the current file first
            if let Some(dir) = &self.current_dir {
                let candidate = dir.join(path);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            // Then search local paths
            for dir in &self.local_paths {
                let candidate = dir.join(path);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        // Search system paths
        for dir in &self.system_paths {
            let candidate = dir.join(path);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Preprocess a source file, resolving all includes recursively.
    pub fn preprocess_file(&mut self, path: &Path, processor: &mut Processor) -> PreprocessResult {
        let canonical = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(e) => {
                return PreprocessResult {
                    text: String::new(),
                    tokens: Vec::new(),
                    errors: vec![format!("cannot open {:?}: {}", path, e)],
                    warnings: Vec::new(),
                };
            }
        };

        let source = if let Some(cached) = self.file_cache.get(&canonical) {
            cached.clone()
        } else {
            match std::fs::read_to_string(&canonical) {
                Ok(s) => {
                    self.file_cache.insert(canonical.clone(), s.clone());
                    s
                }
                Err(e) => {
                    return PreprocessResult {
                        text: String::new(),
                        tokens: Vec::new(),
                        errors: vec![format!("cannot read {:?}: {}", path, e)],
                        warnings: Vec::new(),
                    };
                }
            }
        };

        let old_dir = self.current_dir.take();
        self.current_dir = canonical.parent().map(|p| p.to_path_buf());

        let result = self.preprocess_source(&source, processor);

        self.current_dir = old_dir;
        result
    }

    /// Preprocess source text with include resolution.
    pub fn preprocess_source(
        &mut self,
        source: &str,
        processor: &mut Processor,
    ) -> PreprocessResult {
        let tokens = Lexer::tokenize(source);

        let mut all_errors = Vec::new();
        let mut all_warnings = Vec::new();

        let output = processor.process_with_includes(&tokens, &mut |path, system, macros| {
            self.handle_include(path, system, macros, &mut all_errors, &mut all_warnings)
        });

        all_errors.extend(output.errors);
        all_warnings.extend(output.warnings);

        PreprocessResult {
            text: tokens_to_text(&output.tokens),
            tokens: output.tokens,
            errors: all_errors,
            warnings: all_warnings,
        }
    }

    fn handle_include(
        &mut self,
        path: &str,
        system: bool,
        macros: &mut MacroTable,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> Option<Vec<Token>> {
        if self.depth >= self.max_depth {
            errors.push(format!("include depth exceeded for {:?}", path));
            return Some(Vec::new());
        }

        // Try disk resolution first
        if let Some(resolved) = self.resolve(path, system) {
            if let Some(tokens) =
                self.handle_include_file(&resolved, path, macros, errors, warnings)
            {
                return Some(tokens);
            }
        }

        // Fall back to built-in headers
        if let Some(content) = self.builtin_headers.get(path).cloned() {
            return Some(self.handle_include_source(&content, macros, errors, warnings));
        }

        // Not found anywhere
        None
    }

    fn handle_include_file(
        &mut self,
        resolved: &Path,
        orig_path: &str,
        macros: &mut MacroTable,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> Option<Vec<Token>> {
        let canonical = std::fs::canonicalize(resolved).ok()?;

        // Check #pragma once
        if self.pragma_once_files.contains(&canonical) {
            return Some(Vec::new());
        }

        // Check if this file has an include guard that's already been defined
        if let Some(guard) = self.detect_guard(&canonical) {
            if self.guards.contains(&guard) {
                return Some(Vec::new());
            }
        }

        let source = if let Some(cached) = self.file_cache.get(&canonical) {
            cached.clone()
        } else {
            match std::fs::read_to_string(&canonical) {
                Ok(s) => {
                    self.file_cache.insert(canonical.clone(), s.clone());
                    s
                }
                Err(e) => {
                    errors.push(format!("cannot read {:?}: {}", orig_path, e));
                    return Some(Vec::new());
                }
            }
        };

        let old_dir = self.current_dir.take();
        self.current_dir = canonical.parent().map(|p| p.to_path_buf());
        self.depth += 1;

        let tokens = Lexer::tokenize(&source);

        let mut sub_proc = Processor::with_macros(std::mem::take(macros));
        let output = sub_proc.process_with_includes(&tokens, &mut |p, s, m| {
            self.handle_include(p, s, m, errors, warnings)
        });

        *macros = std::mem::take(sub_proc.macros_mut());

        errors.extend(output.errors);
        warnings.extend(output.warnings);

        self.depth -= 1;
        self.current_dir = old_dir;

        // Track #pragma once
        if output.pragma_once {
            self.pragma_once_files.insert(canonical.clone());
        }

        // If the file had an include guard, mark it
        if let Some(guard) = self.detect_guard(&canonical) {
            if macros.is_defined(&guard) {
                self.guards.insert(guard);
            }
        }

        Some(output.tokens)
    }

    /// Process built-in header source text (no file on disk).
    fn handle_include_source(
        &mut self,
        source: &str,
        macros: &mut MacroTable,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> Vec<Token> {
        self.depth += 1;

        let tokens = Lexer::tokenize(source);

        let mut sub_proc = Processor::with_macros(std::mem::take(macros));
        let output = sub_proc.process_with_includes(&tokens, &mut |p, s, m| {
            self.handle_include(p, s, m, errors, warnings)
        });

        *macros = std::mem::take(sub_proc.macros_mut());

        errors.extend(output.errors);
        warnings.extend(output.warnings);

        self.depth -= 1;

        output.tokens
    }

    /// Detect include guard name for a file.
    fn detect_guard(&self, path: &Path) -> Option<String> {
        let source = self.file_cache.get(path)?;
        let tokens = Lexer::tokenize(source);
        let mut i = 0;

        // Skip leading whitespace/newlines/comments
        while i < tokens.len() {
            match tokens[i].kind {
                TokenKind::Whitespace
                | TokenKind::Newline
                | TokenKind::LineComment
                | TokenKind::BlockComment => i += 1,
                _ => break,
            }
        }

        // Look for #ifndef GUARD
        if i < tokens.len() && tokens[i].kind == TokenKind::Hash {
            i += 1;
            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                i += 1;
            }
            if i < tokens.len() && tokens[i].kind == TokenKind::Ident && tokens[i].text == "ifndef"
            {
                i += 1;
                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                    i += 1;
                }
                if i < tokens.len() && tokens[i].kind == TokenKind::Ident {
                    let guard = tokens[i].text.clone();
                    // Verify next directive is #define GUARD
                    i += 1;
                    while i < tokens.len() {
                        match tokens[i].kind {
                            TokenKind::Whitespace
                            | TokenKind::Newline
                            | TokenKind::LineComment
                            | TokenKind::BlockComment => i += 1,
                            _ => break,
                        }
                    }
                    if i < tokens.len() && tokens[i].kind == TokenKind::Hash {
                        i += 1;
                        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                            i += 1;
                        }
                        if i < tokens.len() && tokens[i].text == "define" {
                            i += 1;
                            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                                i += 1;
                            }
                            if i < tokens.len() && tokens[i].text == guard {
                                return Some(guard);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_local_path() {
        let mut resolver = IncludeResolver::new();
        resolver.add_local_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        let result = resolver.resolve("lib.rs", false);
        assert!(result.is_some());
    }

    #[test]
    fn resolve_system_path() {
        let mut resolver = IncludeResolver::new();
        resolver.add_system_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        let result = resolver.resolve("lib.rs", true);
        assert!(result.is_some());
    }

    #[test]
    fn resolve_relative_to_current() {
        let mut resolver = IncludeResolver::new();
        resolver.current_dir = Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        let result = resolver.resolve("lib.rs", false);
        assert!(result.is_some());
    }

    #[test]
    fn resolve_nonexistent() {
        let resolver = IncludeResolver::new();
        let result = resolver.resolve("nonexistent_file_xyz.h", false);
        assert!(result.is_none());
    }

    #[test]
    fn preprocess_with_include() {
        let dir = std::env::temp_dir().join("pac_test_include");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(dir.join("header.h"), "#define VAL 42\n").unwrap();
        std::fs::write(dir.join("main.c"), "#include \"header.h\"\nint x = VAL;\n").unwrap();

        let mut resolver = IncludeResolver::new();
        let mut proc = Processor::new();
        let result = resolver.preprocess_file(&dir.join("main.c"), &mut proc);

        let text = result.text.trim().to_string();
        assert!(text.contains("int x = 42;"), "got: {}", text);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn include_guard_prevents_double_include() {
        let dir = std::env::temp_dir().join("pac_test_guard2");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(
            dir.join("guarded.h"),
            "#ifndef GUARD_H\n#define GUARD_H\nint guarded;\n#endif\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("main.c"),
            "#include \"guarded.h\"\n#include \"guarded.h\"\n",
        )
        .unwrap();

        let mut resolver = IncludeResolver::new();
        let mut proc = Processor::new();
        let result = resolver.preprocess_file(&dir.join("main.c"), &mut proc);

        let text = result.text.trim().to_string();
        // Should only appear once despite two includes
        assert_eq!(text.matches("int guarded;").count(), 1, "got: {}", text);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_includes() {
        let dir = std::env::temp_dir().join("pac_test_nested_inc");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(dir.join("types.h"), "#define MY_INT int\n").unwrap();
        std::fs::write(
            dir.join("api.h"),
            "#include \"types.h\"\nMY_INT api_func();\n",
        )
        .unwrap();
        std::fs::write(dir.join("main.c"), "#include \"api.h\"\nMY_INT main() {}\n").unwrap();

        let mut resolver = IncludeResolver::new();
        let mut proc = Processor::new();
        let result = resolver.preprocess_file(&dir.join("main.c"), &mut proc);

        let text = result.text.trim().to_string();
        assert!(text.contains("int api_func();"), "got: {}", text);
        assert!(text.contains("int main()"), "got: {}", text);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pragma_once_prevents_double_include() {
        let dir = std::env::temp_dir().join("pac_test_pragma_once");
        let _ = std::fs::create_dir_all(&dir);

        std::fs::write(dir.join("once.h"), "#pragma once\nint once_var;\n").unwrap();
        std::fs::write(
            dir.join("main.c"),
            "#include \"once.h\"\n#include \"once.h\"\n",
        )
        .unwrap();

        let mut resolver = IncludeResolver::new();
        let mut proc = Processor::new();
        let result = resolver.preprocess_file(&dir.join("main.c"), &mut proc);

        let text = result.text.trim().to_string();
        assert_eq!(text.matches("int once_var;").count(), 1, "got: {}", text);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
