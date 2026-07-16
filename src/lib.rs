//! C language frontend: preprocessing, parsing, and checked source contracts.
//!
//! This crate is the frontend stage of the PARC pipeline. It owns:
//!
//! - **Preprocessing**: built-in C preprocessor with macro expansion,
//!   conditional compilation, include resolution, and predefined target macros.
//! - **Parsing**: C11 parser with GNU and Clang extensions, producing a typed AST.
//! - **Contract lowering**: deterministic, crate-private AST normalization
//!   suitable for downstream consumption by linker and codegen stages.
//! - **Source contract**: a serializable checked package (`SourcePackage`) that
//!   captures functions, records, enums, typedefs, variables, macros,
//!   diagnostics, and provenance — independent of parser internals.
//!
//! # Quick start
//!
//! ```
//! use parc::driver::{Config, parse};
//!
//! let config = Config::default();
//! println!("{:?}", parse(&config, "example.c"));
//! ```

#![allow(deprecated)]
#![allow(ellipsis_inclusive_range_patterns)]
#![forbid(unsafe_code)]

pub mod ast;
pub mod contract;
pub mod driver;
mod extract;
pub mod loc;
pub mod parse;
pub mod preprocess;
pub mod print;
pub mod scan;
pub mod span;
pub mod visit;

mod astutil;
mod env;
mod parser;
mod strings;

#[cfg(test)]
mod tests;
