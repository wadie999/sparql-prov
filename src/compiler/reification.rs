//! Compiler-side abstraction over RDF reification schemes.

use oxrdf::Variable;

use crate::model::TriplePattern;

/// Converts a logical provenance-aware triple match into a concrete SPARQL
/// graph pattern.
///
/// A reification scheme is responsible only for locating the identifiers
/// associated with a matched triple. It must not aggregate identifiers or
/// implement provenance algebra.
pub trait ReificationScheme {
    /// Compiles a triple pattern together with the variable that receives
    /// its provenance identifier.
    fn reify(
        &self,
        pattern: &TriplePattern,
        source: &Variable,
    ) -> String;
}