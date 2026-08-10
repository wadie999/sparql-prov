//! Adapters between external SPARQL representations and SPARQLProof.
//!
//! Adapter dependencies are confined to this layer. The core query model,
//! rewriting engine, and rewrite-plan compiler remain independent of any
//! particular SPARQL parser.

mod spargebra;
mod spargebra_expression;

pub use spargebra::{ParsedSparqlQuery, SpargebraAdapterError, parse_sparql};
