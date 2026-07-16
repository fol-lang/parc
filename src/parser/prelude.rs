// Parser implementation maintained directly in Rust.
use self::RuleResult::{Failed, Matched};
use crate::ast::*;
use crate::astutil::*;
use crate::env::{Env, Symbol};
use crate::span::{Node, Span};
fn escape_default(s: &str) -> String {
    s.chars().flat_map(|c| c.escape_default()).collect()
}
fn char_range_at(s: &str, pos: usize) -> (char, usize) {
    let c = &s[pos..].chars().next().unwrap();
    let next_pos = pos + c.len_utf8();
    (*c, next_pos)
}
#[derive(Clone)]
enum RuleResult<T> {
    Matched(usize, T),
    Failed,
}
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub expected: ::std::collections::HashSet<&'static str>,
}
pub type ParseResult<T> = Result<T, ParseError>;

/// One syntax error recovered while parsing a translation unit.
#[derive(PartialEq, Debug, Clone)]
pub struct RecoveryError {
    pub error: ParseError,
    pub skipped: Span,
}

/// Recovered translation unit plus every syntax error and skipped range.
#[derive(PartialEq, Debug, Clone)]
pub struct RecoveredTranslationUnit {
    pub unit: TranslationUnit,
    pub errors: Vec<RecoveryError>,
}
impl ::std::fmt::Display for ParseError {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        r#try!(write!(fmt, "error at {}:{}: expected ", self.line, self.column));
        if self.expected.len() == 0 {
            r#try!(write!(fmt, "EOF"));
        } else if self.expected.len() == 1 {
            r#try!(write!(fmt, "`{}`", escape_default(self.expected.iter().next().unwrap())));
        } else {
            let mut iter = self.expected.iter();
            r#try!(write!(fmt, "one of `{}`", escape_default(iter.next().unwrap())));
            for elem in iter {
                r#try!(write!(fmt, ", `{}`", escape_default(elem)));
            }
        }
        Ok(())
    }
}
impl ::std::error::Error for ParseError {
    fn description(&self) -> &str {
        "parse error"
    }
}
fn slice_eq(input: &str, state: &mut ParseState, pos: usize, m: &'static str) -> RuleResult<()> {
    #![inline]
    #![allow(dead_code)]
    let l = m.len();
    if input.len() >= pos + l && &input.as_bytes()[pos..pos + l] == m.as_bytes() {
        Matched(pos + l, ())
    } else {
        state.mark_failure(pos, m)
    }
}
fn slice_eq_case_insensitive(input: &str, state: &mut ParseState, pos: usize, m: &'static str) -> RuleResult<()> {
    #![inline]
    #![allow(dead_code)]
    let mut used = 0usize;
    let mut input_iter = input[pos..].chars().flat_map(|x| x.to_uppercase());
    for m_char_upper in m.chars().flat_map(|x| x.to_uppercase()) {
        used += m_char_upper.len_utf8();
        let input_char_result = input_iter.next();
        if input_char_result.is_none() || input_char_result.unwrap() != m_char_upper {
            return state.mark_failure(pos, m);
        }
    }
    Matched(pos + used, ())
}
fn any_char(input: &str, state: &mut ParseState, pos: usize) -> RuleResult<()> {
    #![inline]
    #![allow(dead_code)]
    if input.len() > pos {
        let (_, next) = char_range_at(input, pos);
        Matched(next, ())
    } else {
        state.mark_failure(pos, "<character>")
    }
}
fn pos_to_line(input: &str, pos: usize) -> (usize, usize) {
    let before = &input[..pos];
    let line = before.as_bytes().iter().filter(|&&c| c == b'\n').count() + 1;
    let col = before.chars().rev().take_while(|&c| c != '\n').count() + 1;
    (line, col)
}
impl<'input> ParseState<'input> {
    fn mark_failure(&mut self, pos: usize, expected: &'static str) -> RuleResult<()> {
        if self.suppress_fail == 0 {
            if pos > self.max_err_pos {
                self.max_err_pos = pos;
                self.expected.clear();
            }
            if pos == self.max_err_pos {
                self.expected.insert(expected);
            }
        }
        Failed
    }
}
struct ParseState<'input> {
    max_err_pos: usize,
    suppress_fail: usize,
    expected: ::std::collections::HashSet<&'static str>,
    _phantom: ::std::marker::PhantomData<&'input ()>,
    postfix_expression0_cache: ::std::collections::HashMap<usize, RuleResult<Expression>>,
}
impl<'input> ParseState<'input> {
    fn new() -> ParseState<'input> {
        ParseState { max_err_pos: 0, suppress_fail: 0, expected: ::std::collections::HashSet::new(), _phantom: ::std::marker::PhantomData, postfix_expression0_cache: ::std::collections::HashMap::new() }
    }
}

fn __parse__<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<()> {
    #![allow(non_snake_case, unused)]
    {
        __state.suppress_fail += 1;
        let res = {
            let mut __repeat_pos = __pos;
            loop {
                let __pos = __repeat_pos;
                let __step_res = {
                    let __choice_res = {
                        let __seq_res = match slice_eq(__input, __state, __pos, "\r") {
                            Matched(__newpos, _) => Matched(__newpos, ()),
                            Failed => Matched(__pos, ()),
                        };
                        match __seq_res {
                            Matched(__pos, _) => {
                                let __seq_res = slice_eq(__input, __state, __pos, "\n");
                                match __seq_res {
                                    Matched(__pos, _) => match __parse_directive(__input, __state, __pos, env) {
                                        Matched(__newpos, _) => Matched(__newpos, ()),
                                        Failed => Matched(__pos, ()),
                                    },
                                    Failed => Failed,
                                }
                            }
                            Failed => Failed,
                        }
                    };
                    match __choice_res {
                        Matched(__pos, __value) => Matched(__pos, __value),
                        Failed => {
                            if __input.len() > __pos {
                                let (__ch, __next) = char_range_at(__input, __pos);
                                match __ch {
                                    ' ' | '\t' => Matched(__next, ()),
                                    _ => __state.mark_failure(__pos, "[ \t]"),
                                }
                            } else {
                                __state.mark_failure(__pos, "[ \t]")
                            }
                        }
                    }
                };
                match __step_res {
                    Matched(__newpos, __value) => {
                        __repeat_pos = __newpos;
                    }
                    Failed => {
                        break;
                    }
                }
            }
            Matched(__repeat_pos, ())
        };
        __state.suppress_fail -= 1;
        res
    }
}

fn __parse_directive<'input>(__input: &'input str, __state: &mut ParseState<'input>, __pos: usize, env: &mut Env) -> RuleResult<()> {
    #![allow(non_snake_case, unused)]
    {
        let __seq_res = slice_eq(__input, __state, __pos, "#");
        match __seq_res {
            Matched(__pos, _) => {
                let mut __repeat_pos = __pos;
                loop {
                    let __pos = __repeat_pos;
                    let __step_res = if __input.len() > __pos {
                        let (__ch, __next) = char_range_at(__input, __pos);
                        match __ch {
                            '\n' => __state.mark_failure(__pos, "[^\n]"),
                            _ => Matched(__next, ()),
                        }
                    } else {
                        __state.mark_failure(__pos, "[^\n]")
                    };
                    match __step_res {
                        Matched(__newpos, __value) => {
                            __repeat_pos = __newpos;
                        }
                        Failed => {
                            break;
                        }
                    }
                }
                Matched(__repeat_pos, ())
            }
            Failed => Failed,
        }
    }
}
