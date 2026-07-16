use super::token::{Token, TokenKind};

/// Preprocessor tokenizer.
///
/// Splits C source text into preprocessor tokens. This is simpler than
/// a full C lexer — it follows §6.4 of the C standard for preprocessing
/// tokens, which are a superset of C tokens.
pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
    /// True when we are at the logical start of a line (for `#` detection).
    at_line_start: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input: input.as_bytes(),
            pos: 0,
            at_line_start: true,
        }
    }

    /// Tokenize the entire input into a `Vec<Token>`.
    ///
    /// Line continuations (`\` followed by newline) are spliced out before
    /// tokenization, matching C translation phase 2.
    pub fn tokenize(input: &str) -> Vec<Token> {
        let spliced = splice_lines(input);
        let mut lexer = Lexer::new(&spliced);
        let mut tokens = Vec::new();
        loop {
            let tok = lexer.next_token();
            if tok.kind == TokenKind::Eof {
                tokens.push(tok);
                break;
            }
            tokens.push(tok);
        }
        tokens
    }

    pub fn next_token(&mut self) -> Token {
        if self.pos >= self.input.len() {
            return Token {
                kind: TokenKind::Eof,
                text: String::new(),
                offset: self.pos,
            };
        }

        let start = self.pos;
        let b = self.input[self.pos];

        match b {
            b'\n' => {
                self.pos += 1;
                self.at_line_start = true;
                Token {
                    kind: TokenKind::Newline,
                    text: "\n".into(),
                    offset: start,
                }
            }
            b' ' | b'\t' | b'\r' | 0x0C => {
                self.skip_horizontal_whitespace();
                let text = std::str::from_utf8(&self.input[start..self.pos])
                    .unwrap_or(" ")
                    .to_owned();
                Token {
                    kind: TokenKind::Whitespace,
                    text,
                    offset: start,
                }
            }
            b'/' if self.peek(1) == Some(b'/') => {
                self.skip_line_comment();
                let text = std::str::from_utf8(&self.input[start..self.pos])
                    .unwrap_or("//")
                    .to_owned();
                Token {
                    kind: TokenKind::LineComment,
                    text,
                    offset: start,
                }
            }
            b'/' if self.peek(1) == Some(b'*') => {
                self.skip_block_comment();
                let text = std::str::from_utf8(&self.input[start..self.pos])
                    .unwrap_or("/**/")
                    .to_owned();
                Token {
                    kind: TokenKind::BlockComment,
                    text,
                    offset: start,
                }
            }
            b'#' if self.peek(1) == Some(b'#') => {
                self.pos += 2;
                self.at_line_start = false;
                Token {
                    kind: TokenKind::HashHash,
                    text: "##".into(),
                    offset: start,
                }
            }
            b'#' if self.at_line_start => {
                self.pos += 1;
                // at_line_start stays true — the directive body follows
                Token {
                    kind: TokenKind::Hash,
                    text: "#".into(),
                    offset: start,
                }
            }
            b'#' => {
                self.pos += 1;
                self.at_line_start = false;
                // Inside a macro body, # is stringification
                Token {
                    kind: TokenKind::Hash,
                    text: "#".into(),
                    offset: start,
                }
            }
            b'"' => self.lex_string_literal(start),
            b'\'' => self.lex_char_literal(start),
            b'L' | b'u' | b'U' if self.peek(1) == Some(b'"') || self.peek(1) == Some(b'\'') => {
                // Wide/unicode string/char prefixes
                if self.peek(1) == Some(b'"') {
                    self.pos += 1;
                    self.lex_string_literal(start)
                } else {
                    self.pos += 1;
                    self.lex_char_literal(start)
                }
            }
            b'u' if self.peek(1) == Some(b'8') && self.peek(2) == Some(b'"') => {
                self.pos += 2;
                self.lex_string_literal(start)
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_identifier(start),
            b'0'..=b'9' => self.lex_number(start),
            b'.' if self.peek(1).is_some_and(|c| c.is_ascii_digit()) => self.lex_number(start),
            _ => {
                self.at_line_start = false;
                // Multi-character operators
                let next = self.peek(1);
                let len = match (b, next) {
                    (b'<', Some(b'<')) | (b'>', Some(b'>')) => {
                        if self.peek(2) == Some(b'=') {
                            3
                        } else {
                            2
                        }
                    }
                    (b'=', Some(b'='))
                    | (b'!', Some(b'='))
                    | (b'<', Some(b'='))
                    | (b'>', Some(b'='))
                    | (b'&', Some(b'&'))
                    | (b'|', Some(b'|'))
                    | (b'+', Some(b'+'))
                    | (b'-', Some(b'-'))
                    | (b'-', Some(b'>'))
                    | (b'+', Some(b'='))
                    | (b'-', Some(b'='))
                    | (b'*', Some(b'='))
                    | (b'/', Some(b'='))
                    | (b'%', Some(b'='))
                    | (b'&', Some(b'='))
                    | (b'|', Some(b'='))
                    | (b'^', Some(b'=')) => 2,
                    _ => 1,
                };
                self.pos += len;
                let text = std::str::from_utf8(&self.input[start..self.pos])
                    .unwrap_or("?")
                    .to_owned();
                Token {
                    kind: TokenKind::Punct,
                    text,
                    offset: start,
                }
            }
        }
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.input.get(self.pos + ahead).copied()
    }

    fn skip_horizontal_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\r' | 0x0C => self.pos += 1,
                _ => break,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.pos += 2; // skip //
        while self.pos < self.input.len() && self.input[self.pos] != b'\n' {
            self.pos += 1;
        }
    }

    fn skip_block_comment(&mut self) {
        self.pos += 2; // skip /*
        while self.pos + 1 < self.input.len() {
            if self.input[self.pos] == b'*' && self.input[self.pos + 1] == b'/' {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
        // Unterminated — consume rest
        self.pos = self.input.len();
    }

    fn lex_string_literal(&mut self, start: usize) -> Token {
        self.pos += 1; // skip opening "
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'\\' if self.pos + 1 < self.input.len() => {
                    self.pos += 2;
                }
                b'\n' => break, // unterminated
                _ => self.pos += 1,
            }
        }
        self.at_line_start = false;
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("\"\"")
            .to_owned();
        Token {
            kind: TokenKind::StringLiteral,
            text,
            offset: start,
        }
    }

    fn lex_char_literal(&mut self, start: usize) -> Token {
        self.pos += 1; // skip opening '
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'\'' => {
                    self.pos += 1;
                    break;
                }
                b'\\' if self.pos + 1 < self.input.len() => {
                    self.pos += 2;
                }
                b'\n' => break,
                _ => self.pos += 1,
            }
        }
        self.at_line_start = false;
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("''")
            .to_owned();
        Token {
            kind: TokenKind::CharLiteral,
            text,
            offset: start,
        }
    }

    fn lex_identifier(&mut self, start: usize) -> Token {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => self.pos += 1,
                _ => break,
            }
        }
        self.at_line_start = false;
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("")
            .to_owned();
        Token {
            kind: TokenKind::Ident,
            text,
            offset: start,
        }
    }

    fn lex_number(&mut self, start: usize) -> Token {
        // Preprocessing numbers: a superset of C numeric constants.
        // pp-number = digit | . digit | pp-number digit | pp-number ident-char
        //           | pp-number e sign | pp-number E sign | pp-number p sign
        //           | pp-number P sign | pp-number .
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'.' => {
                    self.pos += 1;
                }
                b'+' | b'-'
                    if self.pos > start
                        && matches!(self.input[self.pos - 1], b'e' | b'E' | b'p' | b'P') =>
                {
                    self.pos += 1;
                }
                _ => break,
            }
        }
        self.at_line_start = false;
        let text = std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("")
            .to_owned();
        Token {
            kind: TokenKind::Number,
            text,
            offset: start,
        }
    }
}

/// Splice physical source lines by removing `\` + newline sequences.
/// This implements C translation phase 2.
fn splice_lines(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            i += 2; // skip backslash + newline
        } else if bytes[i] == b'\\'
            && i + 2 < bytes.len()
            && bytes[i + 1] == b'\r'
            && bytes[i + 2] == b'\n'
        {
            i += 3; // skip backslash + \r\n
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_define() {
        let tokens = Lexer::tokenize("#define FOO 42\n");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Hash,
                &TokenKind::Ident,      // define
                &TokenKind::Whitespace, // space
                &TokenKind::Ident,      // FOO
                &TokenKind::Whitespace, // space
                &TokenKind::Number,     // 42
                &TokenKind::Newline,
                &TokenKind::Eof,
            ]
        );
        assert_eq!(tokens[1].text, "define");
        assert_eq!(tokens[3].text, "FOO");
        assert_eq!(tokens[5].text, "42");
    }

    #[test]
    fn lex_string_and_char() {
        let tokens = Lexer::tokenize("\"hello\" 'x'\n");
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
        assert_eq!(tokens[0].text, "\"hello\"");
        assert_eq!(tokens[2].kind, TokenKind::CharLiteral);
        assert_eq!(tokens[2].text, "'x'");
    }

    #[test]
    fn lex_hash_hash() {
        let tokens = Lexer::tokenize("a ## b\n");
        assert_eq!(tokens[0].kind, TokenKind::Ident);
        assert_eq!(tokens[2].kind, TokenKind::HashHash);
        assert_eq!(tokens[4].kind, TokenKind::Ident);
    }

    #[test]
    fn lex_comments() {
        let tokens = Lexer::tokenize("// line\n/* block */\n");
        assert_eq!(tokens[0].kind, TokenKind::LineComment);
        assert_eq!(tokens[2].kind, TokenKind::BlockComment);
    }

    #[test]
    fn lex_pp_number_with_exponent() {
        let tokens = Lexer::tokenize("1e+10 0x1p-3\n");
        assert_eq!(tokens[0].kind, TokenKind::Number);
        assert_eq!(tokens[0].text, "1e+10");
        assert_eq!(tokens[2].kind, TokenKind::Number);
        assert_eq!(tokens[2].text, "0x1p-3");
    }

    #[test]
    fn lex_include_directive() {
        let tokens = Lexer::tokenize("#include <stdio.h>\n");
        assert_eq!(tokens[0].kind, TokenKind::Hash);
        assert_eq!(tokens[1].kind, TokenKind::Ident);
        assert_eq!(tokens[1].text, "include");
    }

    #[test]
    fn lex_escaped_string() {
        let tokens = Lexer::tokenize("\"a\\\"b\"\n");
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
        assert_eq!(tokens[0].text, "\"a\\\"b\"");
    }

    #[test]
    fn lex_line_continuation() {
        // A macro definition split across two lines
        let tokens = Lexer::tokenize("#define FOO \\\n42\n");
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Hash,
                &TokenKind::Ident, // define
                &TokenKind::Whitespace,
                &TokenKind::Ident, // FOO
                &TokenKind::Whitespace,
                &TokenKind::Number, // 42
                &TokenKind::Newline,
                &TokenKind::Eof,
            ]
        );
        assert_eq!(tokens[5].text, "42");
    }

    #[test]
    fn lex_multichar_operators() {
        let tokens = Lexer::tokenize("== != <= >= << >> && || ++ -- ->\n");
        let ops: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Punct)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            ops,
            vec!["==", "!=", "<=", ">=", "<<", ">>", "&&", "||", "++", "--", "->"]
        );
    }
}
