//! Provenance expressions produced by NPCS rewriting.
//!
//! The expression model represents the classical NPCS operators
//! independently of the SPARQL expressions used to construct them.

use std::fmt;

/// Identifier of a provenance source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(String);

impl SourceId {
    /// Creates a provenance source identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A classical NPCS provenance expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProvenanceExpr {
    /// Additive identity.
    Zero,

    /// Multiplicative identity.
    One,

    /// Provenance identifier attached to a graph statement.
    Source(SourceId),

    /// Alternative derivations: `⊕`.
    Sum(Vec<ProvenanceExpr>),

    /// Jointly required provenance: `⊗`.
    Product(Vec<ProvenanceExpr>),

    /// Provenance difference: `⊖`.
    Difference {
        left: Box<ProvenanceExpr>,
        right: Box<ProvenanceExpr>,
    },
}

impl fmt::Display for ProvenanceExpr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("0"),
            Self::One => formatter.write_str("1"),
            Self::Source(source) => source.fmt(formatter),

            Self::Sum(terms) => {
                formatter.write_str("(⊕")?;

                for term in terms {
                    write!(formatter, " {term}")?;
                }

                formatter.write_str(")")
            }

            Self::Product(factors) => {
                formatter.write_str("(⊗")?;

                for factor in factors {
                    write!(formatter, " {factor}")?;
                }

                formatter.write_str(")")
            }

            Self::Difference { left, right } => {
                write!(formatter, "(⊖ {left} {right})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(identifier: &str) -> ProvenanceExpr {
        ProvenanceExpr::Source(SourceId::new(identifier))
    }

    #[test]
    fn source_identifier_preserves_value() {
        let source = SourceId::new("u1");

        assert_eq!(source.as_str(), "u1");
        assert_eq!(source.to_string(), "u1");
    }

    #[test]
    fn renders_identities_and_source() {
        assert_eq!(ProvenanceExpr::Zero.to_string(), "0");
        assert_eq!(ProvenanceExpr::One.to_string(), "1");
        assert_eq!(source("u1").to_string(), "u1");
    }

    #[test]
    fn renders_sum() {
        let expression = ProvenanceExpr::Sum(vec![source("u1"), source("u2")]);

        assert_eq!(expression.to_string(), "(⊕ u1 u2)");
    }

    #[test]
    fn renders_product() {
        let expression = ProvenanceExpr::Product(vec![source("u1"), source("u2")]);

        assert_eq!(expression.to_string(), "(⊗ u1 u2)");
    }

    #[test]
    fn renders_difference() {
        let expression = ProvenanceExpr::Difference {
            left: Box::new(source("u1")),
            right: Box::new(source("u2")),
        };

        assert_eq!(expression.to_string(), "(⊖ u1 u2)");
    }

    #[test]
    fn renders_npcs_running_example() {
        let expression = ProvenanceExpr::Product(vec![
            ProvenanceExpr::Sum(vec![source("u1"), source("u2")]),
            source("u3"),
        ]);

        assert_eq!(expression.to_string(), "(⊗ (⊕ u1 u2) u3)");
    }

    #[test]
    fn preserves_nested_expression_structure() {
        let expression = ProvenanceExpr::Sum(vec![
            source("u1"),
            ProvenanceExpr::Product(vec![source("u2"), source("u3")]),
        ]);

        assert_eq!(expression.to_string(), "(⊕ u1 (⊗ u2 u3))");
    }

    #[test]
    fn renders_empty_sum_and_product_structurally() {
        assert_eq!(ProvenanceExpr::Sum(vec![]).to_string(), "(⊕)");
        assert_eq!(ProvenanceExpr::Product(vec![]).to_string(), "(⊗)");
    }
}
