//! Errors produced while lowering rewrite plans to SPARQL.

use std::error::Error;
use std::fmt;

/// Error raised by the SPARQL compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The current compiler slice does not yet support this plan node.
    UnsupportedPlan(&'static str),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlan(kind) => {
                write!(formatter, "unsupported rewrite plan node: {kind}")
            }
        }
    }
}

impl Error for CompileError {}
