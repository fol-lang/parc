use std::collections::HashMap;

use super::token::{Token, TokenKind};

/// A stored macro definition.
#[derive(Debug, Clone)]
pub struct MacroDef {
    /// Macro name
    pub name: String,
    /// `None` for object-like, `Some(params)` for function-like
    pub params: Option<Vec<String>>,
    /// Whether the macro is variadic (`...`)
    pub is_variadic: bool,
    /// The replacement token list
    pub body: Vec<Token>,
}

/// Macro table: stores and expands macro definitions.
#[derive(Default)]
pub struct MacroTable {
    macros: HashMap<String, MacroDef>,
}

impl MacroTable {
    pub fn new() -> Self {
        MacroTable {
            macros: HashMap::new(),
        }
    }

    pub fn define(&mut self, def: MacroDef) {
        self.macros.insert(def.name.clone(), def);
    }

    pub fn undef(&mut self, name: &str) {
        self.macros.remove(name);
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&MacroDef> {
        self.macros.get(name)
    }

    /// Return all macro definitions.
    pub fn all(&self) -> impl Iterator<Item = &MacroDef> {
        self.macros.values()
    }

    /// Return all macro names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.macros.keys().map(|s| s.as_str())
    }

    /// Number of defined macros.
    pub fn len(&self) -> usize {
        self.macros.len()
    }

    /// Whether the macro table contains no definitions.
    pub fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }

    /// Expand all object-like macros in a token list.
    ///
    /// Performs repeated passes until no more expansions occur.
    /// Tracks a "paint set" to prevent infinite recursion: a macro
    /// currently being expanded cannot expand itself.
    pub fn expand(&self, tokens: &[Token]) -> Vec<Token> {
        self.expand_with_paint(tokens, &mut Vec::new())
    }

    fn expand_with_paint(&self, tokens: &[Token], paint: &mut Vec<String>) -> Vec<Token> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < tokens.len() {
            let tok = &tokens[i];

            if tok.kind == TokenKind::Ident && !paint.contains(&tok.text) {
                if let Some(def) = self.macros.get(&tok.text) {
                    if def.params.is_none() {
                        // Object-like macro — substitute and recurse
                        paint.push(tok.text.clone());
                        let expanded = self.expand_with_paint(&def.body, paint);
                        paint.pop();
                        result.extend(expanded);
                        i += 1;
                        continue;
                    }

                    if def.params.is_some() {
                        // Function-like macro — need to collect arguments
                        if let Some((args, end)) = self.collect_macro_args(tokens, i + 1) {
                            paint.push(tok.text.clone());
                            let expanded_body = self.substitute_fn_macro(def, &args);
                            let expanded = self.expand_with_paint(&expanded_body, paint);
                            paint.pop();
                            result.extend(expanded);
                            i = end;
                            continue;
                        }
                    }
                }
            }

            result.push(tok.clone());
            i += 1;
        }

        result
    }

    /// Collect arguments for a function-like macro invocation.
    /// Returns (args, position after closing paren).
    fn collect_macro_args(
        &self,
        tokens: &[Token],
        start: usize,
    ) -> Option<(Vec<Vec<Token>>, usize)> {
        let mut i = start;
        // Skip whitespace before (
        while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
            i += 1;
        }
        if i >= tokens.len() || tokens[i].text != "(" {
            return None;
        }
        i += 1; // skip (

        let mut args: Vec<Vec<Token>> = vec![Vec::new()];
        let mut depth = 0;

        while i < tokens.len() {
            if tokens[i].text == "(" {
                depth += 1;
                args.last_mut().unwrap().push(tokens[i].clone());
            } else if tokens[i].text == ")" {
                if depth == 0 {
                    i += 1; // skip )
                            // Trim whitespace from args
                    for arg in &mut args {
                        while arg.first().is_some_and(|t| t.kind == TokenKind::Whitespace) {
                            arg.remove(0);
                        }
                        while arg.last().is_some_and(|t| t.kind == TokenKind::Whitespace) {
                            arg.pop();
                        }
                    }
                    // Handle empty single arg: `FOO()` means zero args, not one empty arg
                    if args.len() == 1 && args[0].is_empty() {
                        args.clear();
                    }
                    return Some((args, i));
                }
                depth -= 1;
                args.last_mut().unwrap().push(tokens[i].clone());
            } else if tokens[i].text == "," && depth == 0 {
                args.push(Vec::new());
            } else {
                args.last_mut().unwrap().push(tokens[i].clone());
            }
            i += 1;
        }

        None // unterminated
    }

    /// Substitute parameters in a function-like macro body.
    fn substitute_fn_macro(&self, def: &MacroDef, args: &[Vec<Token>]) -> Vec<Token> {
        let params = match &def.params {
            Some(p) => p,
            None => return def.body.clone(),
        };

        let mut result = Vec::new();
        let mut i = 0;

        while i < def.body.len() {
            let tok = &def.body[i];

            // Check for stringification: # param
            if tok.kind == TokenKind::Hash && i + 1 < def.body.len() {
                let next = &def.body[i + 1];
                // skip whitespace between # and param
                let param_tok = if next.kind == TokenKind::Whitespace && i + 2 < def.body.len() {
                    i += 1;
                    &def.body[i + 1]
                } else {
                    next
                };
                if let Some(idx) = params.iter().position(|p| p == &param_tok.text) {
                    let arg = args.get(idx).cloned().unwrap_or_default();
                    let stringified = stringify_tokens(&arg);
                    result.push(Token {
                        kind: TokenKind::StringLiteral,
                        text: stringified,
                        offset: tok.offset,
                    });
                    i += 2;
                    continue;
                }
            }

            // Check for token pasting: a ## b
            if tok.kind == TokenKind::HashHash {
                // Remove trailing whitespace from result
                while result
                    .last()
                    .is_some_and(|t: &Token| t.kind == TokenKind::Whitespace)
                {
                    result.pop();
                }
                i += 1;
                // Skip whitespace after ##
                while i < def.body.len() && def.body[i].kind == TokenKind::Whitespace {
                    i += 1;
                }
                if i < def.body.len() {
                    let right = &def.body[i];
                    let right_text = if let Some(idx) = params.iter().position(|p| p == &right.text)
                    {
                        let arg = args.get(idx).cloned().unwrap_or_default();
                        tokens_to_string(&arg)
                    } else {
                        right.text.clone()
                    };
                    if let Some(left) = result.last_mut() {
                        left.text.push_str(&right_text);
                    }
                    i += 1;
                }
                continue;
            }

            // Parameter substitution
            if tok.kind == TokenKind::Ident {
                if tok.text == "__VA_ARGS__" && def.is_variadic {
                    // Variadic: args from params.len() onward
                    let va_start = params.len();
                    if va_start < args.len() {
                        for (j, arg) in args[va_start..].iter().enumerate() {
                            if j > 0 {
                                result.push(Token {
                                    kind: TokenKind::Punct,
                                    text: ",".into(),
                                    offset: tok.offset,
                                });
                                result.push(Token {
                                    kind: TokenKind::Whitespace,
                                    text: " ".into(),
                                    offset: tok.offset,
                                });
                            }
                            result.extend(arg.clone());
                        }
                    }
                    i += 1;
                    continue;
                }
                if let Some(idx) = params.iter().position(|p| p == &tok.text) {
                    let arg = args.get(idx).cloned().unwrap_or_default();
                    result.extend(arg);
                    i += 1;
                    continue;
                }
            }

            result.push(tok.clone());
            i += 1;
        }

        result
    }
}

fn stringify_tokens(tokens: &[Token]) -> String {
    let mut s = String::from("\"");
    for t in tokens {
        for ch in t.text.chars() {
            match ch {
                '"' | '\\' => {
                    s.push('\\');
                    s.push(ch);
                }
                _ => s.push(ch),
            }
        }
    }
    s.push('"');
    s
}

fn tokens_to_string(tokens: &[Token]) -> String {
    let mut s = String::new();
    for t in tokens {
        s.push_str(&t.text);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::lexer::Lexer;

    fn tok(kind: TokenKind, text: &str) -> Token {
        Token {
            kind,
            text: text.into(),
            offset: 0,
        }
    }

    fn expand_str(defs: &[(&str, &str)], input: &str) -> String {
        let mut table = MacroTable::new();
        for (name, body) in defs {
            let body_tokens: Vec<Token> = Lexer::tokenize(body)
                .into_iter()
                .filter(|t| t.kind != TokenKind::Eof && t.kind != TokenKind::Newline)
                .collect();
            table.define(MacroDef {
                name: name.to_string(),
                params: None,
                is_variadic: false,
                body: body_tokens,
            });
        }
        let input_tokens: Vec<Token> = Lexer::tokenize(input)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof && t.kind != TokenKind::Newline)
            .collect();
        let expanded = table.expand(&input_tokens);
        tokens_to_string(&expanded)
    }

    #[test]
    fn expand_object_like() {
        let result = expand_str(&[("FOO", "42")], "int x = FOO;\n");
        assert_eq!(result, "int x = 42;");
    }

    #[test]
    fn expand_chained() {
        let result = expand_str(&[("A", "B"), ("B", "42")], "int x = A;\n");
        assert_eq!(result, "int x = 42;");
    }

    #[test]
    fn expand_self_referential_stops() {
        let result = expand_str(&[("X", "X + 1")], "X\n");
        assert_eq!(result, "X + 1");
    }

    #[test]
    fn expand_function_like() {
        let mut table = MacroTable::new();
        table.define(MacroDef {
            name: "ADD".into(),
            params: Some(vec!["a".into(), "b".into()]),
            is_variadic: false,
            body: vec![
                tok(TokenKind::Ident, "a"),
                tok(TokenKind::Whitespace, " "),
                tok(TokenKind::Punct, "+"),
                tok(TokenKind::Whitespace, " "),
                tok(TokenKind::Ident, "b"),
            ],
        });
        let input = vec![
            tok(TokenKind::Ident, "ADD"),
            tok(TokenKind::Punct, "("),
            tok(TokenKind::Number, "1"),
            tok(TokenKind::Punct, ","),
            tok(TokenKind::Whitespace, " "),
            tok(TokenKind::Number, "2"),
            tok(TokenKind::Punct, ")"),
        ];
        let expanded = table.expand(&input);
        let text = tokens_to_string(&expanded);
        assert_eq!(text, "1 + 2");
    }

    #[test]
    fn expand_no_parens_not_function() {
        // Function-like macro without () should not expand
        let mut table = MacroTable::new();
        table.define(MacroDef {
            name: "F".into(),
            params: Some(vec!["x".into()]),
            is_variadic: false,
            body: vec![tok(TokenKind::Ident, "x")],
        });
        let input = vec![tok(TokenKind::Ident, "F")];
        let expanded = table.expand(&input);
        assert_eq!(tokens_to_string(&expanded), "F");
    }
}
