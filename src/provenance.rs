//! Provenance expressions produced by NPCS rewriting.
//!
//! The expression model represents the classical NPCS operators
//! independently of their SPARQL string encoding.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identifier_preserves_value() {
        let source = SourceId::new("u1");

        assert_eq!(source.as_str(), "u1");
        assert_eq!(source.to_string(), "u1");
    }

    #[test]
    fn represents_npcs_running_example() {
        let expression = ProvenanceExpr::Product(vec![
            ProvenanceExpr::Sum(vec![
                ProvenanceExpr::Source(SourceId::new("u1")),
                ProvenanceExpr::Source(SourceId::new("u2")),
            ]),
            ProvenanceExpr::Source(SourceId::new("u3")),
        ]);

        assert_eq!(
            expression,
            ProvenanceExpr::Product(vec![
                ProvenanceExpr::Sum(vec![
                    ProvenanceExpr::Source(SourceId::new("u1")),
                    ProvenanceExpr::Source(SourceId::new("u2")),
                ]),
                ProvenanceExpr::Source(SourceId::new("u3")),
            ])
        );
    }

    #[test]
    fn represents_difference() {
        let expression = ProvenanceExpr::Difference {
            left: Box::new(ProvenanceExpr::Source(SourceId::new("u1"))),
            right: Box::new(ProvenanceExpr::Source(SourceId::new("u2"))),
        };

        assert!(matches!(expression, ProvenanceExpr::Difference { .. }));
    }
}
