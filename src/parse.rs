use crate::ast::{Constant, Declaration, Expression, Statement, TranslationUnit};
use crate::driver::Flavor;
use crate::env::Env;
use crate::parser;
use crate::span::Node;

pub use crate::parser::ParseResult;
pub use crate::parser::{ParseError, RecoveredTranslationUnit, RecoveryError};

fn env_for(flavor: Flavor) -> Env {
    match flavor {
        Flavor::StdC11 => Env::with_core(),
        Flavor::GnuC11 => Env::with_gnu(),
        Flavor::ClangC11 => Env::with_clang(),
    }
}

pub fn constant(source: &str, flavor: Flavor) -> ParseResult<Constant> {
    let mut env = env_for(flavor);
    parser::constant(source, &mut env)
}

pub fn expression(source: &str, flavor: Flavor) -> ParseResult<Box<Node<Expression>>> {
    let mut env = env_for(flavor);
    parser::expression(source, &mut env)
}

pub fn declaration(source: &str, flavor: Flavor) -> ParseResult<Node<Declaration>> {
    let mut env = env_for(flavor);
    parser::declaration(source, &mut env)
}

pub fn statement(source: &str, flavor: Flavor) -> ParseResult<Box<Node<Statement>>> {
    let mut env = env_for(flavor);
    parser::statement(source, &mut env)
}

pub fn translation_unit(source: &str, flavor: Flavor) -> ParseResult<TranslationUnit> {
    let mut env = env_for(flavor);
    parser::translation_unit(source, &mut env)
}

pub fn translation_unit_resilient(source: &str, flavor: Flavor) -> RecoveredTranslationUnit {
    let mut env = env_for(flavor);
    parser::translation_unit_resilient(source, &mut env)
}
