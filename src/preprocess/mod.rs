//! Built-in C preprocessor
//!
//! Provides tokenization, directive parsing, macro expansion, and
//! conditional compilation without requiring an external `gcc -E` invocation.

mod builtins;
mod directive;
mod expr;
mod include;
mod lexer;
mod macros;
mod predefined;
mod processor;
mod token;

pub use self::builtins::builtin_headers;
pub(crate) use self::directive::expanded_header_name;
pub use self::directive::{parse_directive, Directive};
pub use self::expr::eval_condition;
pub use self::include::{IncludeResolver, PreprocessResult};
pub use self::lexer::Lexer;
pub use self::macros::{MacroDef, MacroTable};
pub use self::predefined::{define_target_macros, Target, TargetArch, TargetOs};
pub use self::processor::{preprocess, tokens_to_text, IncludeRequest, Processor, ProcessorOutput};
pub use self::token::{Token, TokenKind};
