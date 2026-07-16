//! Preprocess and parse C source file into an abstract syntax tree

use std::collections::HashSet;
use std::error;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::Command;

use crate::ast::TranslationUnit;
use crate::env::Env;
use crate::loc;
use crate::parser::{translation_unit, translation_unit_resilient};

/// Parser configuration
#[derive(Clone, Debug)]
pub struct Config {
    /// Command used to invoke C preprocessor
    pub cpp_command: String,
    /// Options to pass to the preprocessor program
    pub cpp_options: Vec<String>,
    /// Language flavor to parse
    pub flavor: Flavor,
}

impl Config {
    /// Use `gcc` as a pre-processor and enable gcc extensions
    pub fn with_gcc() -> Config {
        Config {
            cpp_command: "gcc".into(),
            cpp_options: vec!["-E".into()],
            flavor: Flavor::GnuC11,
        }
    }

    /// Use `clang` as a pre-processor and enable Clang extensions
    pub fn with_clang() -> Config {
        Config {
            cpp_command: "clang".into(),
            cpp_options: vec!["-E".into()],
            flavor: Flavor::ClangC11,
        }
    }
}

impl Default for Config {
    #[cfg(target_os = "macos")]
    fn default() -> Config {
        Self::with_clang()
    }

    #[cfg(not(target_os = "macos"))]
    fn default() -> Config {
        Self::with_gcc()
    }
}

/// C language flavors
#[derive(Copy, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Flavor {
    /// Strict standard C11
    StdC11,
    /// Standard C11 with GNU extensions
    GnuC11,
    /// Standard C11 with Clang extensions
    ClangC11,
}

/// Result of a successful parse
#[derive(Clone, Debug)]
pub struct Parse {
    /// Pre-processed source text
    pub source: String,
    /// Root of the abstract syntax tree
    pub unit: TranslationUnit,
}

#[derive(Debug)]
/// Error type returned from `parse`
pub enum Error {
    PreprocessorError(io::Error),
    SyntaxError(SyntaxError),
}

impl From<SyntaxError> for Error {
    fn from(e: SyntaxError) -> Error {
        Error::SyntaxError(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::PreprocessorError(e) => write!(fmt, "preprocessor error: {}", e),
            Error::SyntaxError(e) => write!(fmt, "syntax error: {}", e),
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::PreprocessorError(_) => "preprocessor error",
            Error::SyntaxError(_) => "syntax error",
        }
    }
}

/// Syntax error during parsing
#[derive(Debug, Clone)]
pub struct SyntaxError {
    /// Pre-processed source text
    pub source: String,
    /// Line number in the preprocessed source
    pub line: usize,
    /// Column number in the preprocessed source
    pub column: usize,
    /// Byte position in the preproccessed source
    pub offset: usize,
    /// Tokens expected at the error location
    pub expected: HashSet<&'static str>,
}

impl SyntaxError {
    /// Quoted and comma-separated list of expected tokens
    pub fn format_expected(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let mut list = self.expected.iter().collect::<Vec<_>>();
        list.sort();
        for (i, t) in list.iter().enumerate() {
            if i > 0 {
                r#try!(write!(fmt, ", "));
            }
            r#try!(write!(fmt, "'{}'", t));
        }

        Ok(())
    }

    pub fn get_location(&self) -> (loc::Location<'_>, Vec<loc::Location<'_>>) {
        loc::get_location_for_offset(&self.source, self.offset)
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let (loc, inc) = self.get_location();
        r#try!(write!(
            fmt,
            "unexpected token at \"{}\" line {} column {}, expected ",
            loc.file, loc.line, self.column
        ));
        r#try!(self.format_expected(fmt));
        for loc in inc {
            r#try!(write!(fmt, "\n  included from {}:{}", loc.file, loc.line));
        }
        Ok(())
    }
}

/// Parse a C file
pub fn parse<P: AsRef<Path>>(config: &Config, source: P) -> Result<Parse, Error> {
    let processed = match preprocess(config, source.as_ref()) {
        Ok(s) => s,
        Err(e) => return Err(Error::PreprocessorError(e)),
    };

    Ok(r#try!(parse_preprocessed(config, processed)))
}

pub fn parse_preprocessed(config: &Config, source: String) -> Result<Parse, SyntaxError> {
    let mut env = match config.flavor {
        Flavor::StdC11 => Env::with_core(),
        Flavor::GnuC11 => Env::with_gnu(),
        Flavor::ClangC11 => Env::with_clang(),
    };

    match translation_unit(&source, &mut env) {
        Ok(unit) => Ok(Parse { source, unit }),
        Err(err) => Err(SyntaxError {
            source,
            line: err.line,
            column: err.column,
            offset: err.offset,
            expected: err.expected,
        }),
    }
}

pub fn parse_preprocessed_resilient(config: &Config, source: String) -> Parse {
    let mut env = match config.flavor {
        Flavor::StdC11 => Env::with_core(),
        Flavor::GnuC11 => Env::with_gnu(),
        Flavor::ClangC11 => Env::with_clang(),
    };

    let unit = translation_unit_resilient(&source, &mut env);
    Parse { source, unit }
}

/// Parse a C file using the built-in preprocessor (no external gcc/clang needed).
///
/// Include paths should be provided in `include_paths`. The target is
/// auto-detected from the host platform.
pub fn parse_builtin<P: AsRef<Path>>(
    config: &Config,
    source: P,
    include_paths: &[&Path],
) -> Result<Parse, Error> {
    use crate::preprocess::{
        define_target_macros, tokens_to_text, IncludeResolver, Processor, Target,
    };

    let target = Target::host();
    let mut table = crate::preprocess::MacroTable::new();
    define_target_macros(&mut table, &target);

    let mut processor = Processor::with_macros(table);
    let mut resolver = IncludeResolver::new();

    for path in include_paths {
        resolver.add_system_path(path);
        resolver.add_local_path(path);
    }

    let result = resolver.preprocess_file(source.as_ref(), &mut processor);

    if !result.errors.is_empty() {
        return Err(Error::PreprocessorError(io::Error::other(
            result.errors.join("\n"),
        )));
    }

    let source_text = tokens_to_text(&result.tokens);
    Ok(parse_preprocessed(config, source_text)?)
}

/// Extract all macros defined after preprocessing a C file.
///
/// This is the built-in equivalent of `gcc -dD -E`: it preprocesses the
/// file and returns every `#define` that was active at the end of
/// preprocessing, including predefined target macros and macros from
/// included headers.
pub fn capture_macros<P: AsRef<Path>>(
    source: P,
    include_paths: &[&Path],
) -> Result<Vec<(String, String)>, Error> {
    use crate::preprocess::{
        define_target_macros, tokens_to_text, IncludeResolver, Processor, Target,
    };

    let target = Target::host();
    let mut table = crate::preprocess::MacroTable::new();
    define_target_macros(&mut table, &target);

    let mut processor = Processor::with_macros(table);
    let mut resolver = IncludeResolver::new();

    for path in include_paths {
        resolver.add_system_path(path);
        resolver.add_local_path(path);
    }

    let result = resolver.preprocess_file(source.as_ref(), &mut processor);

    if !result.errors.is_empty() {
        return Err(Error::PreprocessorError(io::Error::other(
            result.errors.join("\n"),
        )));
    }

    let mut macros: Vec<(String, String)> = processor
        .macros()
        .all()
        .map(|def| {
            let value = tokens_to_text(&def.body);
            (def.name.clone(), value.trim().to_string())
        })
        .collect();
    macros.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(macros)
}

fn preprocess(config: &Config, source: &Path) -> io::Result<String> {
    let mut cmd = Command::new(&config.cpp_command);

    for item in &config.cpp_options {
        cmd.arg(item);
    }

    cmd.arg(source);

    let output = r#try!(cmd.output());

    if output.status.success() {
        match String::from_utf8(output.stdout) {
            Ok(s) => Ok(s),
            Err(e) => Err(io::Error::other(e)),
        }
    } else {
        match String::from_utf8(output.stderr) {
            Ok(s) => Err(io::Error::other(s)),
            Err(_) => Err(io::Error::other("cpp error contains invalid utf-8")),
        }
    }
}
