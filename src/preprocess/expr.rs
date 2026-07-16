use super::macros::MacroTable;
use super::token::{Token, TokenKind};

/// Evaluate a `#if` constant expression.
///
/// Supports:
/// - Integer literals (decimal, octal, hex)
/// - `defined(NAME)` and `defined NAME`
/// - Unary: `!`, `-`, `+`, `~`
/// - Binary: `+`, `-`, `*`, `/`, `%`, `<<`, `>>`, `<`, `>`, `<=`, `>=`,
///   `==`, `!=`, `&`, `^`, `|`, `&&`, `||`
/// - Ternary: `? :`
/// - Parentheses
/// - Character constants: `'x'`
///
/// Any undefined identifier evaluates to `0` (per C standard §6.10.1p4).
pub fn eval_condition(tokens: &[Token], macros: &MacroTable) -> bool {
    // First expand macros (except `defined` operator)
    let expanded = expand_for_condition(tokens, macros);
    let mut parser = ExprParser::new(&expanded);
    let result = parser.ternary();
    result != 0
}

/// Expand tokens for #if evaluation.
/// The `defined` operator must be handled before macro expansion.
fn expand_for_condition(tokens: &[Token], macros: &MacroTable) -> Vec<Token> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if tokens[i].kind == TokenKind::Ident && tokens[i].text == "defined" {
            i += 1;
            // Skip whitespace
            while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                i += 1;
            }
            if i < tokens.len() && tokens[i].text == "(" {
                i += 1;
                while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                    i += 1;
                }
                if i < tokens.len() && tokens[i].kind == TokenKind::Ident {
                    let val = if macros.is_defined(&tokens[i].text) {
                        1
                    } else {
                        0
                    };
                    result.push(Token {
                        kind: TokenKind::Number,
                        text: val.to_string(),
                        offset: tokens[i].offset,
                    });
                    i += 1;
                    while i < tokens.len() && tokens[i].kind == TokenKind::Whitespace {
                        i += 1;
                    }
                    if i < tokens.len() && tokens[i].text == ")" {
                        i += 1;
                    }
                }
            } else if i < tokens.len() && tokens[i].kind == TokenKind::Ident {
                let val = if macros.is_defined(&tokens[i].text) {
                    1
                } else {
                    0
                };
                result.push(Token {
                    kind: TokenKind::Number,
                    text: val.to_string(),
                    offset: tokens[i].offset,
                });
                i += 1;
            }
        } else {
            result.push(tokens[i].clone());
            i += 1;
        }
    }

    // Now expand macros in the result
    let expanded = macros.expand(&result);

    // Replace any remaining identifiers with 0 (C standard)
    expanded
        .into_iter()
        .map(|t| {
            if t.kind == TokenKind::Ident {
                Token {
                    kind: TokenKind::Number,
                    text: "0".into(),
                    offset: t.offset,
                }
            } else {
                t
            }
        })
        .collect()
}

struct ExprParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        ExprParser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if self.tokens[i].kind != TokenKind::Whitespace {
                return Some(&self.tokens[i]);
            }
            i += 1;
        }
        None
    }

    fn advance(&mut self) -> Option<&Token> {
        while self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            if tok.kind != TokenKind::Whitespace {
                return Some(tok);
            }
        }
        None
    }

    fn ternary(&mut self) -> i128 {
        let cond = self.logical_or();
        if self.peek().is_some_and(|t| t.text == "?") {
            self.advance(); // skip ?
            let then_val = self.ternary();
            if self.peek().is_some_and(|t| t.text == ":") {
                self.advance(); // skip :
            }
            let else_val = self.ternary();
            if cond != 0 {
                then_val
            } else {
                else_val
            }
        } else {
            cond
        }
    }

    fn logical_or(&mut self) -> i128 {
        let mut val = self.logical_and();
        while self.peek_text() == "||" {
            self.advance();
            let rhs = self.logical_and();
            val = if val != 0 || rhs != 0 { 1 } else { 0 };
        }
        val
    }

    fn logical_and(&mut self) -> i128 {
        let mut val = self.bitwise_or();
        while self.peek_text() == "&&" {
            self.advance();
            let rhs = self.bitwise_or();
            val = if val != 0 && rhs != 0 { 1 } else { 0 };
        }
        val
    }

    fn bitwise_or(&mut self) -> i128 {
        let mut val = self.bitwise_xor();
        while self.peek_text() == "|" && !self.peek_text_is("||") {
            self.advance();
            val |= self.bitwise_xor();
        }
        val
    }

    fn bitwise_xor(&mut self) -> i128 {
        let mut val = self.bitwise_and();
        while self.peek_text() == "^" {
            self.advance();
            val ^= self.bitwise_and();
        }
        val
    }

    fn bitwise_and(&mut self) -> i128 {
        let mut val = self.equality();
        while self.peek_text() == "&" && !self.peek_text_is("&&") {
            self.advance();
            val &= self.equality();
        }
        val
    }

    fn equality(&mut self) -> i128 {
        let mut val = self.relational();
        loop {
            let op = self.peek_text().to_owned();
            match op.as_str() {
                "==" => {
                    self.advance();
                    val = if val == self.relational() { 1 } else { 0 };
                }
                "!=" => {
                    self.advance();
                    val = if val != self.relational() { 1 } else { 0 };
                }
                _ => break,
            }
        }
        val
    }

    fn relational(&mut self) -> i128 {
        let mut val = self.shift();
        loop {
            let op = self.peek_text().to_owned();
            match op.as_str() {
                "<" if !self.peek_text_is("<<") => {
                    self.advance();
                    val = if val < self.shift() { 1 } else { 0 };
                }
                ">" if !self.peek_text_is(">>") => {
                    self.advance();
                    val = if val > self.shift() { 1 } else { 0 };
                }
                "<=" => {
                    self.advance();
                    val = if val <= self.shift() { 1 } else { 0 };
                }
                ">=" => {
                    self.advance();
                    val = if val >= self.shift() { 1 } else { 0 };
                }
                _ => break,
            }
        }
        val
    }

    fn shift(&mut self) -> i128 {
        let mut val = self.additive();
        loop {
            let op = self.peek_text().to_owned();
            match op.as_str() {
                "<<" => {
                    self.advance();
                    val <<= self.additive();
                }
                ">>" => {
                    self.advance();
                    val >>= self.additive();
                }
                _ => break,
            }
        }
        val
    }

    fn additive(&mut self) -> i128 {
        let mut val = self.multiplicative();
        loop {
            let op = self.peek_text().to_owned();
            match op.as_str() {
                "+" => {
                    self.advance();
                    val += self.multiplicative();
                }
                "-" => {
                    self.advance();
                    val -= self.multiplicative();
                }
                _ => break,
            }
        }
        val
    }

    fn multiplicative(&mut self) -> i128 {
        let mut val = self.unary();
        loop {
            let op = self.peek_text().to_owned();
            match op.as_str() {
                "*" => {
                    self.advance();
                    val *= self.unary();
                }
                "/" => {
                    self.advance();
                    let rhs = self.unary();
                    val = if rhs != 0 { val / rhs } else { 0 };
                }
                "%" => {
                    self.advance();
                    let rhs = self.unary();
                    val = if rhs != 0 { val % rhs } else { 0 };
                }
                _ => break,
            }
        }
        val
    }

    fn unary(&mut self) -> i128 {
        let op = self.peek_text().to_owned();
        match op.as_str() {
            "!" => {
                self.advance();
                if self.unary() == 0 {
                    1
                } else {
                    0
                }
            }
            "-" => {
                self.advance();
                -self.unary()
            }
            "+" => {
                self.advance();
                self.unary()
            }
            "~" => {
                self.advance();
                !self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> i128 {
        if self.peek_text() == "(" {
            self.advance(); // skip (
            let val = self.ternary();
            if self.peek_text() == ")" {
                self.advance(); // skip )
            }
            return val;
        }

        let tok = match self.advance() {
            Some(t) => t.clone(),
            None => return 0,
        };

        match tok.kind {
            TokenKind::Number => parse_integer(&tok.text),
            TokenKind::CharLiteral => parse_char_constant(&tok.text),
            _ => 0,
        }
    }

    fn peek_text(&self) -> &str {
        self.peek().map_or("", |t| &t.text)
    }

    fn peek_text_is(&self, s: &str) -> bool {
        // Check if the *actual* token text matches (for multi-char operators)
        self.peek().is_some_and(|t| t.text == s)
    }
}

fn parse_integer(s: &str) -> i128 {
    let s = s.trim_end_matches(['u', 'U', 'l', 'L']);
    if s.starts_with("0x") || s.starts_with("0X") {
        i128::from_str_radix(&s[2..], 16).unwrap_or(0)
    } else if s.starts_with("0b") || s.starts_with("0B") {
        i128::from_str_radix(&s[2..], 2).unwrap_or(0)
    } else if s.starts_with('0') && s.len() > 1 && s.chars().all(|c| c.is_ascii_digit()) {
        i128::from_str_radix(&s[1..], 8).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

fn parse_char_constant(s: &str) -> i128 {
    // Strip quotes
    let inner = &s[1..s.len() - 1];
    if inner.starts_with('\\') {
        match inner.chars().nth(1) {
            Some('n') => 10,
            Some('t') => 9,
            Some('r') => 13,
            Some('0') => 0,
            Some('\\') => b'\\' as i128,
            Some('\'') => b'\'' as i128,
            Some(c) => c as i128,
            None => 0,
        }
    } else {
        inner.chars().next().map_or(0, |c| c as i128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocess::lexer::Lexer;

    fn eval(src: &str) -> bool {
        let tokens: Vec<Token> = Lexer::tokenize(src)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof && t.kind != TokenKind::Newline)
            .collect();
        let macros = MacroTable::new();
        eval_condition(&tokens, &macros)
    }

    fn eval_with(defs: &[&str], src: &str) -> bool {
        let mut macros = MacroTable::new();
        for &name in defs {
            macros.define(super::super::macros::MacroDef {
                name: name.into(),
                params: None,
                is_variadic: false,
                body: vec![Token {
                    kind: TokenKind::Number,
                    text: "1".into(),
                    offset: 0,
                }],
            });
        }
        let tokens: Vec<Token> = Lexer::tokenize(src)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof && t.kind != TokenKind::Newline)
            .collect();
        eval_condition(&tokens, &macros)
    }

    #[test]
    fn test_simple_true() {
        assert!(eval("1"));
    }

    #[test]
    fn test_simple_false() {
        assert!(!eval("0"));
    }

    #[test]
    fn test_arithmetic() {
        assert!(eval("2 + 3 == 5"));
        assert!(!eval("2 + 3 == 6"));
    }

    #[test]
    fn test_logical() {
        assert!(eval("1 && 1"));
        assert!(!eval("1 && 0"));
        assert!(eval("0 || 1"));
        assert!(!eval("0 || 0"));
    }

    #[test]
    fn test_negation() {
        assert!(eval("!0"));
        assert!(!eval("!1"));
    }

    #[test]
    fn test_defined() {
        assert!(eval_with(&["FOO"], "defined(FOO)"));
        assert!(!eval_with(&["FOO"], "defined(BAR)"));
        assert!(eval_with(&["FOO"], "defined FOO"));
    }

    #[test]
    fn test_hex_literal() {
        assert!(eval("0xFF == 255"));
    }

    #[test]
    fn test_ternary() {
        assert!(eval("1 ? 42 : 0"));
        assert!(!eval("0 ? 1 : 0"));
    }

    #[test]
    fn test_comparison() {
        assert!(eval("3 > 2"));
        assert!(eval("2 < 3"));
        assert!(eval("3 >= 3"));
        assert!(eval("3 <= 3"));
    }

    #[test]
    fn test_shift() {
        assert!(eval("1 << 3 == 8"));
    }

    #[test]
    fn test_undefined_ident_is_zero() {
        assert!(!eval("UNDEFINED_THING"));
    }

    #[test]
    fn test_char_constant() {
        assert!(eval("'A' == 65"));
    }

    #[test]
    fn test_suffix_and_large_limit_literals() {
        assert!(eval("4294967295U > 2147483647"));
        assert!(eval("18446744073709551615ULL > 0"));
        assert!(eval("0xffffffffffffffffULL == 18446744073709551615ULL"));
    }

    #[test]
    fn test_negative_limit_style_literals() {
        assert!(eval("(-9223372036854775807LL - 1LL) < 0"));
        assert!(eval("(-2147483647 - 1) < 0"));
    }

    #[test]
    fn test_mixed_suffix_arithmetic_and_shift() {
        assert!(eval("(1UL << 31) == 2147483648UL"));
        assert!(eval("(0xffU & 0xf0U) == 0xf0U"));
    }
}
