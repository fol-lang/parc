use super::token::{Token, TokenKind};

/// A parsed preprocessor directive.
#[derive(Debug, Clone, PartialEq)]
pub enum Directive {
    /// `#define NAME body...`
    Define {
        name: String,
        params: Option<Vec<String>>,
        is_variadic: bool,
        body: Vec<Token>,
    },
    /// `#undef NAME`
    Undef { name: String },
    /// `#include "path"`, `#include <path>`, or `#include_next <path>`.
    ///
    /// `next` resumes the search *after* the root the including file came
    /// from, which is how a compiler header chains to the libc one of the
    /// same name: GCC ships `stdint.h` and hands off to glibc's.
    Include {
        path: String,
        system: bool,
        next: bool,
    },
    /// `#if expr`
    If { tokens: Vec<Token> },
    /// `#ifdef NAME`
    Ifdef { name: String },
    /// `#ifndef NAME`
    Ifndef { name: String },
    /// `#elif expr`
    Elif { tokens: Vec<Token> },
    /// `#else`
    Else,
    /// `#endif`
    Endif,
    /// `#error message`
    Error { message: String },
    /// `#warning message`
    Warning { message: String },
    /// `#pragma tokens`
    Pragma { tokens: Vec<Token> },
    /// `#line number ["file"]`
    Line { line: u32, file: Option<String> },
    /// `# number "file"` (gcc line marker)
    LineMarker {
        line: u32,
        file: String,
        flags: Vec<u32>,
    },
    /// Empty `#` followed by newline
    Null,
    /// Unknown directive
    Unknown { name: String, tokens: Vec<Token> },
}

/// Parse a directive from a token slice that starts with `#`.
///
/// The input should be the tokens from after the `#` up to (not including) the
/// newline. Whitespace tokens between `#` and the directive name are skipped.
pub fn parse_directive(tokens: &[Token]) -> Directive {
    let mut i = 0;

    // Skip whitespace after #
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }

    if i >= tokens.len() {
        return Directive::Null;
    }

    // gcc line markers: # 123 "file" ...
    if tokens[i].kind == TokenKind::Number {
        return parse_line_marker(&tokens[i..]);
    }

    if tokens[i].kind != TokenKind::Ident {
        return Directive::Unknown {
            name: String::new(),
            tokens: tokens.to_vec(),
        };
    }

    let name = &tokens[i].text;
    let rest = &tokens[i + 1..];

    match name.as_str() {
        "define" => parse_define(rest),
        "undef" => parse_undef(rest),
        "include" => parse_include(rest, false),
        // `#include_next` resumes after the root the including file came from.
        "include_next" => parse_include(rest, true),
        "if" => Directive::If {
            tokens: strip_whitespace_edges(rest),
        },
        "ifdef" => parse_ifdef(rest),
        "ifndef" => parse_ifndef(rest),
        "elif" => Directive::Elif {
            tokens: strip_whitespace_edges(rest),
        },
        "else" => Directive::Else,
        "endif" => Directive::Endif,
        "error" => Directive::Error {
            message: collect_text(rest),
        },
        "warning" => Directive::Warning {
            message: collect_text(rest),
        },
        "pragma" => Directive::Pragma {
            tokens: strip_whitespace_edges(rest),
        },
        "line" => parse_line(rest),
        _ => Directive::Unknown {
            name: name.clone(),
            tokens: rest.to_vec(),
        },
    }
}

fn parse_define(tokens: &[Token]) -> Directive {
    let mut i = 0;
    // Skip whitespace
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }
    if i >= tokens.len() || tokens[i].kind != TokenKind::Ident {
        return Directive::Unknown {
            name: "define".into(),
            tokens: tokens.to_vec(),
        };
    }
    let name = tokens[i].text.clone();
    i += 1;

    // Check for function-like macro: `(` must immediately follow name (no space)
    let (params, is_variadic) = if i < tokens.len() && tokens[i].text == "(" {
        i += 1; // skip (
        let mut params = Vec::new();
        let mut variadic = false;
        loop {
            // skip whitespace
            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                i += 1;
            }
            if i >= tokens.len() {
                break;
            }
            if tokens[i].text == ")" {
                i += 1;
                break;
            }
            if tokens[i].text == "."
                && i + 2 < tokens.len()
                && tokens[i + 1].text == "."
                && tokens[i + 2].text == "."
            {
                variadic = true;
                i += 3;
                // skip to )
                while i < tokens.len() && tokens[i].text != ")" {
                    i += 1;
                }
                if i < tokens.len() {
                    i += 1;
                }
                break;
            }
            if tokens[i].kind == TokenKind::Ident {
                params.push(tokens[i].text.clone());
                i += 1;
            }
            // skip whitespace and comma
            while i < tokens.len()
                && (tokens[i].kind == TokenKind::Whitespace || tokens[i].text == ",")
            {
                i += 1;
            }
        }
        (Some(params), variadic)
    } else {
        (None, false)
    };

    // Skip leading whitespace in body
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }

    let body = tokens[i..].to_vec();

    Directive::Define {
        name,
        params,
        is_variadic,
        body,
    }
}

fn parse_undef(tokens: &[Token]) -> Directive {
    let name = first_ident(tokens);
    match name {
        Some(n) => Directive::Undef { name: n },
        None => Directive::Unknown {
            name: "undef".into(),
            tokens: tokens.to_vec(),
        },
    }
}

fn parse_include(tokens: &[Token], next: bool) -> Directive {
    let text = collect_text(tokens);
    let trimmed = text.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        Directive::Include {
            path: trimmed[1..trimmed.len() - 1].to_owned(),
            system: true,
            next,
        }
    } else if trimmed.starts_with('"') && trimmed.ends_with('"') {
        Directive::Include {
            path: trimmed[1..trimmed.len() - 1].to_owned(),
            system: false,
            next,
        }
    } else {
        // Macro-expanded include — return as-is
        Directive::Include {
            path: trimmed.to_owned(),
            system: false,
            next,
        }
    }
}

fn parse_ifdef(tokens: &[Token]) -> Directive {
    match first_ident(tokens) {
        Some(name) => Directive::Ifdef { name },
        None => Directive::Unknown {
            name: "ifdef".into(),
            tokens: tokens.to_vec(),
        },
    }
}

fn parse_ifndef(tokens: &[Token]) -> Directive {
    match first_ident(tokens) {
        Some(name) => Directive::Ifndef { name },
        None => Directive::Unknown {
            name: "ifndef".into(),
            tokens: tokens.to_vec(),
        },
    }
}

fn parse_line(tokens: &[Token]) -> Directive {
    let mut i = 0;
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }
    if i >= tokens.len() || tokens[i].kind != TokenKind::Number {
        return Directive::Unknown {
            name: "line".into(),
            tokens: tokens.to_vec(),
        };
    }
    let line: u32 = tokens[i].text.parse().unwrap_or(0);
    i += 1;
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }
    let file = if i < tokens.len() && tokens[i].kind == TokenKind::StringLiteral {
        let s = &tokens[i].text;
        Some(s[1..s.len() - 1].to_owned())
    } else {
        None
    };
    Directive::Line { line, file }
}

fn parse_line_marker(tokens: &[Token]) -> Directive {
    let mut i = 0;
    if tokens[i].kind != TokenKind::Number {
        return Directive::Null;
    }
    let line: u32 = tokens[i].text.parse().unwrap_or(0);
    i += 1;
    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
        i += 1;
    }
    let file = if i < tokens.len() && tokens[i].kind == TokenKind::StringLiteral {
        let s = &tokens[i].text;
        let f = s[1..s.len() - 1].to_owned();
        i += 1;
        f
    } else {
        return Directive::Line { line, file: None };
    };
    let mut flags = Vec::new();
    while i < tokens.len() {
        if tokens[i].kind == TokenKind::Whitespace {
            i += 1;
            continue;
        }
        if tokens[i].kind == TokenKind::Number {
            if let Ok(f) = tokens[i].text.parse() {
                flags.push(f);
            }
        }
        i += 1;
    }
    Directive::LineMarker { line, file, flags }
}

fn first_ident(tokens: &[Token]) -> Option<String> {
    for t in tokens {
        if t.kind == TokenKind::Whitespace {
            continue;
        }
        if t.kind == TokenKind::Ident {
            return Some(t.text.clone());
        }
        return None;
    }
    None
}

/// Joins a directive's tokens, with each comment standing for the one space it
/// is replaced by.
///
/// Comments are whitespace, as `TokenKind::LineComment` and
/// `TokenKind::BlockComment` say. Keeping their text made a trailing comment
/// part of the header name: `#include <stdarg.h> /* why */` asked for a file
/// spelled `<stdarg.h> /* why */`, which no search root has. `sqlite3.h`
/// spells its `<stdarg.h>` exactly that way.
fn collect_text(tokens: &[Token]) -> String {
    let mut text = String::new();
    for token in tokens {
        let piece = match token.kind {
            TokenKind::LineComment | TokenKind::BlockComment => " ",
            _ => token.text.as_str(),
        };
        if text.is_empty() && piece.trim().is_empty() {
            continue;
        }
        text.push_str(piece);
    }
    text.truncate(text.trim_end().len());
    text
}

fn strip_whitespace_edges(tokens: &[Token]) -> Vec<Token> {
    let mut start = 0;
    while start < tokens.len() && tokens[start].kind == TokenKind::Whitespace {
        start += 1;
    }
    let mut end = tokens.len();
    while end > start && tokens[end - 1].kind == TokenKind::Whitespace {
        end -= 1;
    }
    tokens[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::lexer::Lexer;

    fn directive_from(src: &str) -> Directive {
        let tokens = Lexer::tokenize(src);
        // Find the # token, take everything between it and newline
        let hash_idx = tokens.iter().position(|t| t.kind == TokenKind::Hash);
        let hash_idx = hash_idx.expect("no # found");
        let newline_idx = tokens[hash_idx..]
            .iter()
            .position(|t| t.kind == TokenKind::Newline)
            .map(|i| i + hash_idx)
            .unwrap_or(tokens.len());
        parse_directive(&tokens[hash_idx + 1..newline_idx])
    }

    #[test]
    fn test_define_object() {
        match directive_from("#define FOO 42\n") {
            Directive::Define {
                name, params, body, ..
            } => {
                assert_eq!(name, "FOO");
                assert!(params.is_none());
                assert_eq!(body.len(), 1);
                assert_eq!(body[0].text, "42");
            }
            other => panic!("expected Define, got {:?}", other),
        }
    }

    #[test]
    fn test_define_function() {
        match directive_from("#define MAX(a, b) ((a)>(b)?(a):(b))\n") {
            Directive::Define {
                name,
                params,
                is_variadic,
                ..
            } => {
                assert_eq!(name, "MAX");
                assert_eq!(params, Some(vec!["a".into(), "b".into()]));
                assert!(!is_variadic);
            }
            other => panic!("expected Define, got {:?}", other),
        }
    }

    #[test]
    fn test_include_system() {
        match directive_from("#include <stdio.h>\n") {
            Directive::Include { path, system, .. } => {
                assert_eq!(path, "stdio.h");
                assert!(system);
            }
            other => panic!("expected Include, got {:?}", other),
        }
    }

    #[test]
    fn test_include_local() {
        match directive_from("#include \"myheader.h\"\n") {
            Directive::Include { path, system, .. } => {
                assert_eq!(path, "myheader.h");
                assert!(!system);
            }
            other => panic!("expected Include, got {:?}", other),
        }
    }

    /// A comment after the header name is not part of the header name.
    ///
    /// `sqlite3.h` line 35 is `#include <stdarg.h>     /* Needed for the
    /// definition of va_list */`, and reading the comment as part of the name
    /// made the whole header unbindable.
    #[test]
    fn test_include_ignores_a_trailing_comment() {
        for source in [
            "#include <stdarg.h> /* Needed for the definition of va_list */\n",
            "#include <stdarg.h>     // why\n",
            "#include /* here */ <stdarg.h>\n",
        ] {
            match directive_from(source) {
                Directive::Include { path, system, .. } => {
                    assert_eq!(path, "stdarg.h", "{source:?}");
                    assert!(system, "{source:?}");
                }
                other => panic!("expected Include for {source:?}, got {other:?}"),
            }
        }

        match directive_from("#include \"local.h\" /* and a comment */\n") {
            Directive::Include { path, system, .. } => {
                assert_eq!(path, "local.h");
                assert!(!system);
            }
            other => panic!("expected Include, got {other:?}"),
        }
    }

    #[test]
    fn test_ifdef() {
        match directive_from("#ifdef FOO\n") {
            Directive::Ifdef { name } => assert_eq!(name, "FOO"),
            other => panic!("expected Ifdef, got {:?}", other),
        }
    }

    #[test]
    fn test_ifndef() {
        match directive_from("#ifndef GUARD_H\n") {
            Directive::Ifndef { name } => assert_eq!(name, "GUARD_H"),
            other => panic!("expected Ifndef, got {:?}", other),
        }
    }

    #[test]
    fn test_else_endif() {
        assert_eq!(directive_from("#else\n"), Directive::Else);
        assert_eq!(directive_from("#endif\n"), Directive::Endif);
    }

    #[test]
    fn test_undef() {
        match directive_from("#undef FOO\n") {
            Directive::Undef { name } => assert_eq!(name, "FOO"),
            other => panic!("expected Undef, got {:?}", other),
        }
    }

    #[test]
    fn test_error() {
        match directive_from("#error \"something went wrong\"\n") {
            Directive::Error { message } => {
                assert!(message.contains("something went wrong"));
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_line_marker() {
        match directive_from("# 42 \"foo.h\" 1 3\n") {
            Directive::LineMarker { line, file, flags } => {
                assert_eq!(line, 42);
                assert_eq!(file, "foo.h");
                assert_eq!(flags, vec![1, 3]);
            }
            other => panic!("expected LineMarker, got {:?}", other),
        }
    }

    #[test]
    fn test_null_directive() {
        assert_eq!(directive_from("#\n"), Directive::Null);
    }
}
