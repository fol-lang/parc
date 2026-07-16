//! C language frontend: preprocessing, parsing, and source-level semantic extraction.
//!
//! This crate is the frontend stage of the PARC pipeline. It owns:
//!
//! - **Preprocessing**: built-in C preprocessor with macro expansion,
//!   conditional compilation, include resolution, and predefined target macros.
//! - **Parsing**: C11 parser with GNU and Clang extensions, producing a typed AST.
//! - **Extraction**: source-level declaration normalization into a durable IR
//!   suitable for downstream consumption by linker and codegen stages.
//! - **Source IR**: a serializable frontend contract (`SourcePackage`) that
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

pub mod ast;
pub mod driver;
pub mod extract;
pub mod intake;
pub mod ir;
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
