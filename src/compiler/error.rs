//! Errors produced while lowering rewrite plans to SPARQL.

use std::error::Error;
use std::fmt;

/// Error raised by the SPARQL compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The compiler does not yet support this plan node.
    UnsupportedPlan(&'static str),

    /// The rewrite plan violates a structural compiler invariant.
    InvalidPlan(&'static str),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlan(kind) => {
                write!(formatter, "unsupported rewrite plan node: {kind}")
            }

            Self::InvalidPlan(reason) => {
                write!(formatter, "invalid rewrite plan: {reason}")
            }
        }
    }
}

impl Error for CompileError {}
