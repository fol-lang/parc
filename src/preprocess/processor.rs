use super::directive::{parse_directive, Directive};
use super::expr::eval_condition;
use super::macros::{MacroDef, MacroTable};
use super::token::{Token, TokenKind};

/// Preprocessor processor: handles conditional compilation, macro
/// definitions, and produces the output token stream.
///
/// This is the engine that walks through lexed tokens, evaluates
/// `#if`/`#ifdef`/`#ifndef`/`#elif`/`#else`/`#endif` blocks, processes
/// `#define`/`#undef`, and emits the surviving tokens with macros expanded.
pub struct Processor {
    macros: MacroTable,
}

/// Result of processing: the output tokens plus any diagnostics.
pub struct ProcessorOutput {
    /// The output tokens after conditional compilation and macro expansion.
    pub tokens: Vec<Token>,
    /// Any `#error` directives encountered in active branches.
    pub errors: Vec<String>,
    /// Any `#warning` directives encountered in active branches.
    pub warnings: Vec<String>,
    /// Include requests found in active branches.
    pub includes: Vec<IncludeRequest>,
    /// Whether `#pragma once` was encountered.
    pub pragma_once: bool,
}

/// An `#include` directive found during processing.
#[derive(Debug, Clone)]
pub struct IncludeRequest {
    /// The path from the directive.
    pub path: String,
    /// Whether this is a `<system>` include.
    pub system: bool,
    /// Source offset of the directive.
    pub offset: usize,
}

/// State of a conditional block.
#[derive(Debug, Clone, Copy)]
struct CondState {
    /// Whether any branch in this #if/#elif chain has been taken.
    any_taken: bool,
    /// Whether the current branch is active (emitting tokens).
    active: bool,
    /// Whether the enclosing scope is active.
    parent_active: bool,
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor {
    pub fn new() -> Self {
        Processor {
            macros: MacroTable::new(),
        }
    }

    /// Create a processor with pre-defined macros.
    pub fn with_macros(macros: MacroTable) -> Self {
        Processor { macros }
    }

    /// Get a reference to the macro table.
    pub fn macros(&self) -> &MacroTable {
        &self.macros
    }

    /// Process a token stream, resolving conditionals and expanding macros.
    pub fn process(&mut self, tokens: &[Token]) -> ProcessorOutput {
        self.process_inner(tokens, &mut |_, _, _| None)
    }

    /// Process tokens with inline include resolution.
    ///
    /// The `include_handler` is called for each `#include` directive in an
    /// active branch. It receives `(path, system, &mut self)` and should
    /// return `Some(tokens)` if the include was resolved, or `None` to skip.
    pub fn process_with_includes<F>(
        &mut self,
        tokens: &[Token],
        include_handler: &mut F,
    ) -> ProcessorOutput
    where
        F: FnMut(&str, bool, &mut MacroTable) -> Option<Vec<Token>>,
    {
        self.process_inner(tokens, include_handler)
    }

    /// Get a mutable reference to the macro table.
    pub fn macros_mut(&mut self) -> &mut MacroTable {
        &mut self.macros
    }

    fn process_inner<F>(&mut self, tokens: &[Token], include_handler: &mut F) -> ProcessorOutput
    where
        F: FnMut(&str, bool, &mut MacroTable) -> Option<Vec<Token>>,
    {
        let mut output = ProcessorOutput {
            tokens: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            includes: Vec::new(),
            pragma_once: false,
        };

        let mut cond_stack: Vec<CondState> = Vec::new();
        let mut i = 0;

        while i < tokens.len() {
            // Check if current scope is active
            let active = cond_stack.last().is_none_or(|s| s.active);

            // Look for directives: # at start of line
            if tokens[i].kind == TokenKind::Hash {
                // Collect tokens until newline
                let dir_start = i;
                i += 1;
                let mut dir_tokens = Vec::new();
                while i < tokens.len() && tokens[i].kind != TokenKind::Newline {
                    dir_tokens.push(tokens[i].clone());
                    i += 1;
                }
                // Skip the newline
                if i < tokens.len() && tokens[i].kind == TokenKind::Newline {
                    i += 1;
                }

                let directive = parse_directive(&dir_tokens);

                match directive {
                    Directive::If {
                        tokens: expr_tokens,
                    } => {
                        let parent_active = active;
                        let branch_active = if parent_active {
                            eval_condition(&expr_tokens, &self.macros)
                        } else {
                            false
                        };
                        cond_stack.push(CondState {
                            any_taken: branch_active,
                            active: branch_active && parent_active,
                            parent_active,
                        });
                    }
                    Directive::Ifdef { name } => {
                        let parent_active = active;
                        let branch_active = if parent_active {
                            self.macros.is_defined(&name)
                        } else {
                            false
                        };
                        cond_stack.push(CondState {
                            any_taken: branch_active,
                            active: branch_active && parent_active,
                            parent_active,
                        });
                    }
                    Directive::Ifndef { name } => {
                        let parent_active = active;
                        let branch_active = if parent_active {
                            !self.macros.is_defined(&name)
                        } else {
                            false
                        };
                        cond_stack.push(CondState {
                            any_taken: branch_active,
                            active: branch_active && parent_active,
                            parent_active,
                        });
                    }
                    Directive::Elif {
                        tokens: expr_tokens,
                    } => {
                        if let Some(state) = cond_stack.last_mut() {
                            if state.any_taken || !state.parent_active {
                                state.active = false;
                            } else {
                                let branch_active = eval_condition(&expr_tokens, &self.macros);
                                state.active = branch_active;
                                if branch_active {
                                    state.any_taken = true;
                                }
                            }
                        }
                    }
                    Directive::Else => {
                        if let Some(state) = cond_stack.last_mut() {
                            if state.any_taken || !state.parent_active {
                                state.active = false;
                            } else {
                                state.active = true;
                                state.any_taken = true;
                            }
                        }
                    }
                    Directive::Endif => {
                        cond_stack.pop();
                    }
                    _ if !active => {
                        // Skip non-conditional directives in inactive branches
                    }
                    Directive::Define {
                        name,
                        params,
                        is_variadic,
                        body,
                    } => {
                        self.macros.define(MacroDef {
                            name,
                            params,
                            is_variadic,
                            body,
                        });
                    }
                    Directive::Undef { name } => {
                        self.macros.undef(&name);
                    }
                    Directive::Include { path, system } => {
                        // Try inline include resolution first
                        if let Some(inc_tokens) = include_handler(&path, system, &mut self.macros) {
                            output.tokens.extend(inc_tokens);
                        } else {
                            output.includes.push(IncludeRequest {
                                path,
                                system,
                                offset: tokens[dir_start].offset,
                            });
                        }
                    }
                    Directive::Error { message } => {
                        output.errors.push(message);
                    }
                    Directive::Warning { message } => {
                        output.warnings.push(message);
                    }
                    Directive::Pragma { ref tokens } => {
                        // Check for #pragma once
                        let first_nonws = tokens.iter().find(|t| t.kind != TokenKind::Whitespace);
                        if first_nonws.is_some_and(|t| t.text == "once") {
                            output.pragma_once = true;
                        }
                    }
                    Directive::Line { .. }
                    | Directive::LineMarker { .. }
                    | Directive::Null
                    | Directive::Unknown { .. } => {
                        // Silently skip
                    }
                }
                continue;
            }

            // Skip tokens in inactive branches
            if !active {
                i += 1;
                continue;
            }

            // Skip comments and newlines in output
            match tokens[i].kind {
                TokenKind::LineComment | TokenKind::BlockComment => {
                    i += 1;
                    continue;
                }
                TokenKind::Newline => {
                    // Emit newline as a newline character so downstream text
                    // preserves line boundaries (needed for parsing multi-line output).
                    output.tokens.push(Token {
                        kind: TokenKind::Whitespace,
                        text: "\n".into(),
                        offset: tokens[i].offset,
                    });
                    i += 1;
                    continue;
                }
                _ => {}
            }

            output.tokens.push(tokens[i].clone());
            i += 1;
        }

        // Expand macros in the output
        output.tokens = self.macros.expand(&output.tokens);

        output
    }
}

/// Convenience: preprocess source text and return the output text.
pub fn preprocess(source: &str) -> ProcessorOutput {
    let tokens = super::lexer::Lexer::tokenize(source);
    let mut processor = Processor::new();
    processor.process(&tokens)
}

/// Convert a token stream to a string.
pub fn tokens_to_text(tokens: &[Token]) -> String {
    let mut s = String::new();
    for t in tokens {
        s.push_str(&t.text);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(src: &str) -> String {
        let out = preprocess(src);
        tokens_to_text(&out.tokens).trim().to_string()
    }

    fn pp_with_defs(defs: &[(&str, &str)], src: &str) -> String {
        let tokens = super::super::lexer::Lexer::tokenize(src);
        let mut proc = Processor::new();
        for &(name, value) in defs {
            let body_tokens: Vec<Token> = super::super::lexer::Lexer::tokenize(value)
                .into_iter()
                .filter(|t| t.kind != TokenKind::Eof && t.kind != TokenKind::Newline)
                .collect();
            proc.macros.define(MacroDef {
                name: name.into(),
                params: None,
                is_variadic: false,
                body: body_tokens,
            });
        }
        let out = proc.process(&tokens);
        tokens_to_text(&out.tokens).trim().to_string()
    }

    #[test]
    fn ifdef_defined() {
        let src = "\
#define FOO
#ifdef FOO
int x;
#endif
";
        assert_eq!(pp(src), "int x;");
    }

    #[test]
    fn ifdef_undefined() {
        let src = "\
#ifdef FOO
int x;
#endif
";
        assert_eq!(pp(src), "");
    }

    #[test]
    fn ifndef_defined() {
        let src = "\
#define FOO
#ifndef FOO
int x;
#endif
";
        assert_eq!(pp(src), "");
    }

    #[test]
    fn ifndef_undefined() {
        let src = "\
#ifndef FOO
int x;
#endif
";
        assert_eq!(pp(src), "int x;");
    }

    #[test]
    fn ifdef_else() {
        let src = "\
#ifdef FOO
int x;
#else
int y;
#endif
";
        assert_eq!(pp(src), "int y;");
    }

    #[test]
    fn if_true() {
        let src = "\
#if 1
int x;
#endif
";
        assert_eq!(pp(src), "int x;");
    }

    #[test]
    fn if_false() {
        let src = "\
#if 0
int x;
#endif
";
        assert_eq!(pp(src), "");
    }

    #[test]
    fn if_elif_else() {
        let src = "\
#if 0
int a;
#elif 1
int b;
#else
int c;
#endif
";
        assert_eq!(pp(src), "int b;");
    }

    #[test]
    fn if_elif_else_none_taken() {
        let src = "\
#if 0
int a;
#elif 0
int b;
#else
int c;
#endif
";
        assert_eq!(pp(src), "int c;");
    }

    #[test]
    fn nested_conditionals() {
        let src = "\
#define OUTER
#ifdef OUTER
#ifdef INNER
int a;
#else
int b;
#endif
#endif
";
        assert_eq!(pp(src), "int b;");
    }

    #[test]
    fn define_and_use() {
        let src = "\
#define SIZE 42
int buf[SIZE];
";
        assert_eq!(pp(src), "int buf[42];");
    }

    #[test]
    fn undef_removes_macro() {
        let src = "\
#define FOO 1
#undef FOO
#ifdef FOO
int x;
#endif
";
        assert_eq!(pp(src), "");
    }

    #[test]
    fn if_defined_expr() {
        let src = "\
#define FEAT
#if defined(FEAT)
int x;
#endif
";
        assert_eq!(pp(src), "int x;");
    }

    #[test]
    fn include_requests_collected() {
        let src = "\
#include <stdio.h>
#include \"local.h\"
int x;
";
        let out = preprocess(src);
        assert_eq!(out.includes.len(), 2);
        assert_eq!(out.includes[0].path, "stdio.h");
        assert!(out.includes[0].system);
        assert_eq!(out.includes[1].path, "local.h");
        assert!(!out.includes[1].system);
    }

    #[test]
    fn error_directive_collected() {
        let src = "\
#error \"oops\"
int x;
";
        let out = preprocess(src);
        assert_eq!(out.errors.len(), 1);
        assert!(out.errors[0].contains("oops"));
    }

    #[test]
    fn inactive_branch_skips_error() {
        let src = "\
#if 0
#error \"should not fire\"
#endif
int x;
";
        let out = preprocess(src);
        assert!(out.errors.is_empty());
        assert_eq!(tokens_to_text(&out.tokens).trim(), "int x;");
    }

    #[test]
    fn predefined_macros() {
        let result = pp_with_defs(
            &[("LINUX", "1")],
            "\
#ifdef LINUX
int linux;
#else
int other;
#endif
",
        );
        assert_eq!(result, "int linux;");
    }

    #[test]
    fn include_guard_pattern() {
        let src = "\
#ifndef MY_HEADER_H
#define MY_HEADER_H
int guarded;
#endif
";
        assert_eq!(pp(src), "int guarded;");
    }

    #[test]
    fn if_arithmetic() {
        let src = "\
#define VER 3
#if VER >= 2
int new_api;
#else
int old_api;
#endif
";
        assert_eq!(pp(src), "int new_api;");
    }

    #[test]
    fn inactive_nested_does_not_leak() {
        let src = "\
#if 0
#if 1
int leaked;
#endif
#endif
";
        assert_eq!(pp(src), "");
    }
}
